//! Shared test support: an in-process mock Dart VM Service.
//!
//! Serves canned responses for `getVM` and the `ext.flutter.inspector.*`
//! extensions over a real WebSocket, so the JSON-RPC protocol layer can be
//! exercised end-to-end without an emulator or a Flutter app.

#![allow(dead_code)]

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message as WsMessage;

pub const ROOT_TREE: &str = r#"{
  "valueId": "inspector-0",
  "description": "MaterialApp",
  "name": "MaterialApp",
  "type": "_ElementDiagnosticsNode",
  "summaryTree": true,
  "children": [
    {
      "valueId": "inspector-1",
      "description": "Scaffold",
      "name": "Scaffold",
      "type": "_ElementDiagnosticsNode",
      "summaryTree": true,
      "children": [
        {
          "valueId": "inspector-2",
          "description": "Center",
          "name": "Center",
          "type": "_ElementDiagnosticsNode",
          "summaryTree": true,
          "children": [
            {
              "valueId": "inspector-3",
              "description": "Text(\"Olá, sniff\")",
              "name": "Text",
              "type": "_ElementDiagnosticsNode",
              "summaryTree": true,
              "children": []
            }
          ]
        }
      ]
    }
  ]
}"#;

pub fn default_properties() -> HashMap<String, &'static str> {
    let mut m = HashMap::new();
    m.insert("inspector-0".into(), "[]");
    m.insert(
        "inspector-1".into(),
        r#"[{"name":"backgroundColor","propertyType":"Color","description":"Color(0xff2563eb)"}]"#,
    );
    m.insert(
        "inspector-2".into(),
        r#"[{"name":"alignment","value":"center","propertyType":"AlignmentGeometry","description":"AlignmentGeometry.center"}]"#,
    );
    m.insert(
        "inspector-3".into(),
        r#"[
          {"name":"data","value":"Olá, sniff","propertyType":"String","description":"Olá, sniff"},
          {"name":"color","propertyType":"Color","description":"Color(0xffffffff)"},
          {"name":"fontSize","value":"16.0","propertyType":"double","description":"16.0"},
          {"name":"fontWeight","value":"400.0","propertyType":"double","description":"400.0"}
        ]"#,
    );
    m
}

fn respond(method: &str, params: &Value, props: &HashMap<String, &'static str>) -> Value {
    match method {
        "getVM" => json!({
            "result": {
                "isolates": [
                    {"id": "isolates/123", "name": "main", "number": "1"}
                ]
            }
        }),
        "ext.flutter.inspector.getRootWidgetSummaryTree" => {
            let tree: Value = serde_json::from_str(ROOT_TREE).expect("tree");
            json!({ "result": tree })
        }
        "ext.flutter.inspector.getLayoutExplorerNode" => {
            let id = params.get("id").and_then(Value::as_str).unwrap_or("");
            match id {
                "inspector-3" => json!({
                    "result": {
                        "isBox": true,
                        "size": {"width": "100.0", "height": "20.0"},
                        "parentData": {"offsetX": "12.0", "offsetY": "8.0"}
                    }
                }),
                "inspector-2" => json!({
                    "result": {
                        "isBox": true,
                        "size": {"width": "300.0", "height": "200.0"},
                        "parentData": {"offsetX": "0.0", "offsetY": "0.0"}
                    }
                }),
                _ => json!({ "result": {
                    "size": {"width": "400.0", "height": "400.0"},
                    "parentData": {"offsetX": "0.0", "offsetY": "0.0"}
                } }),
            }
        }
        "ext.flutter.inspector.getProperties" => {
            let id = params.get("arg").and_then(Value::as_str).unwrap_or("");
            let props = props.get(id).copied().unwrap_or("[]");
            let arr: Value = serde_json::from_str(props).expect("props");
            json!({ "result": arr })
        }
        "ext.flutter.timeDilation" => json!({
            "result": { "timeDilation": params.get("timeDilation").cloned().unwrap_or(Value::Null) }
        }),
        _ => json!({ "error": { "code": -32601, "message": "unhandled mock method" } }),
    }
}

/// Spawn a mock VM Service on an ephemeral port; returns its address.
pub async fn spawn_mock_vm_service(props: HashMap<String, &'static str>) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let props = props.clone();
            tokio::spawn(handle_ws(stream, props));
        }
    });
    addr
}

async fn handle_ws(stream: TcpStream, props: HashMap<String, &'static str>) {
    let ws = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(_) => return,
    };
    let (mut sink, mut source) = ws.split();
    while let Some(msg) = source.next().await {
        let Ok(WsMessage::Text(text)) = msg else {
            continue;
        };
        let Ok(req) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let id = req.get("id").cloned();
        let method = req.get("method").and_then(Value::as_str).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(Value::Null);
        let mut resp = respond(method, &params, &props);
        if let Some(id) = id {
            resp.as_object_mut().map(|m| m.insert("id".into(), id));
        }
        let frame = WsMessage::Text(serde_json::to_string(&resp).unwrap().into());
        if sink.send(frame).await.is_err() {
            break;
        }
    }
}

/// Capture the mock widget tree through `extract` and return the roots.
pub async fn capture(ws_uri: &str, depth: usize) -> Vec<sniff_flutter::ElementSnapshot> {
    let inspector = sniff_flutter::FlutterInspector::connect(ws_uri)
        .await
        .expect("connect");
    let roots = sniff_flutter::extractor::extract(&inspector, depth)
        .await
        .expect("extract");
    inspector.close().await;
    roots
}
