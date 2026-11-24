  function strOrNull(v) {
    return typeof v === 'string' ? v : null;
  }

  // ---- v2 global facets (captured once, in the same JS pass) -------------
  // `meta` keys are sorted here so the emitted map is already in the same
  // order the Rust BTreeMap parser will use (byte-identical output).
  function collectMeta() {
    const raw = {};
    const metas = document.querySelectorAll('meta');
    for (let i = 0; i < metas.length; i++) {
      const m = metas[i];
      let key = strOrNull(m.getAttribute('name'));
      let value = strOrNull(m.getAttribute('content'));
      if (!key && m.hasAttribute('charset')) {
        key = 'charset';
        value = strOrNull(m.getAttribute('charset'));
      }
      if (!key) key = strOrNull(m.getAttribute('http-equiv'));
      if (key) raw[key] = value === null ? '' : value;
    }
    const out = {};
    Object.keys(raw).sort().forEach(function (k) { out[k] = raw[k]; });
    return out;
  }

  function collectLinks() {
    const out = [];
    const links = document.querySelectorAll('link');
    for (let i = 0; i < links.length; i++) {
      const l = links[i];
      out.push({
        rel: l.getAttribute('rel') || '',
        href: strOrNull(l.getAttribute('href')),
        as: strOrNull(l.getAttribute('as')),
        fetchpriority: strOrNull(l.getAttribute('fetchpriority')),
        integrity: strOrNull(l.getAttribute('integrity')),
        crossorigin: strOrNull(l.getAttribute('crossorigin'))
      });
    }
    return out;
  }

  function collectScripts() {
    const out = [];
    const scripts = document.querySelectorAll('script');
    for (let i = 0; i < scripts.length; i++) {
      const s = scripts[i];
      let text = null;
      try {
        if (!s.getAttribute('src') && s.textContent) {
          text = s.textContent.replace(/\s+/g, ' ').trim().slice(0, 200);
        }
      } catch (e) {}
      out.push({
        src: strOrNull(s.getAttribute('src')),
        type: strOrNull(s.getAttribute('type')),
        defer: s.hasAttribute('defer'),
        async: s.hasAttribute('async'),
        fetchpriority: strOrNull(s.getAttribute('fetchpriority')),
        integrity: strOrNull(s.getAttribute('integrity')),
        blocking: s.hasAttribute('blocking'),
        in_head: !!(s.parentElement && s.parentElement.tagName === 'HEAD'),
        text: text,
        importance: strOrNull(s.getAttribute('importance'))
      });
    }
    return out;
  }

  function collectStyleSheets() {
    const out = [];
    let sheets;
    try { sheets = document.styleSheets; } catch (e) { return out; }
    for (let i = 0; i < sheets.length; i++) {
      const sh = sheets[i];
      let blocked = false;
      try { const rules = sh.cssRules; } catch (e) { blocked = true; }
      let href = null;
      let media = 'all';
      try { href = strOrNull(sh.href); } catch (e) {}
      try { if (sh.media && sh.media.mediaText) media = sh.media.mediaText; } catch (e) {}
      out.push({ href: href, media: media, blocked: blocked });
    }
    return out;
  }

  function collectDocumentFacet() {
    let secure = null;
    try { if (typeof window.isSecureContext === 'boolean') secure = window.isSecureContext; } catch (e) {}
    return {
      title: document.title || null,
      lang: document.documentElement ? strOrNull(document.documentElement.getAttribute('lang')) : null,
      origin: strOrNull(window.location && window.location.origin),
      doctype: document.doctype ? strOrNull(document.doctype.name) : null,
      meta: collectMeta(),
      links: collectLinks(),
      scripts: collectScripts(),
      style_sheets: collectStyleSheets(),
      is_secure_context: secure
    };
  }

  // Closed value sets from docs/modernization/03-facets.md F-C. We probe
  // each candidate with matchMedia instead of serializing a raw
  // MediaQueryList, so the emitted map is stable across browser versions.
  function mediaCanonical(name, values, fallback) {
    for (let i = 0; i < values.length; i++) {
      try {
        if (window.matchMedia('(' + name + ': ' + values[i] + ')').matches) return values[i];
      } catch (e) {}
    }
    return fallback;
  }

  function collectEnv() {
    const media = {
      'prefers-color-scheme': mediaCanonical('prefers-color-scheme', ['dark', 'light'], 'no-preference'),
      'prefers-reduced-motion': mediaCanonical('prefers-reduced-motion', ['reduce'], 'no-preference'),
      'prefers-contrast': mediaCanonical('prefers-contrast', ['more', 'less', 'custom'], 'no-preference'),
      'forced-colors': mediaCanonical('forced-colors', ['active'], 'none'),
      'pointer': mediaCanonical('pointer', ['fine', 'coarse'], 'none'),
      'hover': mediaCanonical('hover', ['hover'], 'none'),
      'any-pointer': mediaCanonical('any-pointer', ['fine', 'coarse'], 'none'),
      'any-hover': mediaCanonical('any-hover', ['hover'], 'none')
    };
    const emulated = {};
    const requested = opts.emulate || {};
    Object.keys(requested).sort().forEach(function (k) { emulated[k] = String(requested[k]); });
    let dpr = 1;
    let width = 0;
    let height = 0;
    try { dpr = window.devicePixelRatio || 1; } catch (e) {}
    try { width = window.innerWidth || 0; height = window.innerHeight || 0; } catch (e) {}
    return {
      media: media,
      emulated: emulated,
      device: {
        dpr: Math.round(dpr * 100) / 100,
        width: Math.round(width),
        height: Math.round(height)
      }
    };
  }

  function styleObj(style) {
    const out = {};
    if (!style) return out;
    for (let i = 0; i < style.length; i++) {
      const name = style[i];
      out[name] = style.getPropertyValue(name);
    }
    return out;
  }

  function atRuleText(rule) {
    try {
      if (typeof rule.conditionText === 'string' && rule.conditionText) {
        const ctor = rule.constructor ? rule.constructor.name : '';
        if (ctor === 'CSSSupportsRule') return '@supports ' + rule.conditionText;
        if (ctor === 'CSSMediaRule') return '@media ' + rule.conditionText;
        return rule.conditionText;
      }
    } catch (e) {}
    try { if (rule.media && rule.media.mediaText) return '@media ' + rule.media.mediaText; } catch (e) {}
    try { if (typeof rule.cssText === 'string') return rule.cssText.split('{')[0].trim(); } catch (e) {}
    return '';
  }

  function ruleMatches(el, selector) {
    try { return el.matches(selector); } catch (e) { return false; }
  }

  // CSSOM origin for the captured elements only (bounded by declaredLimit).
  // Cross-origin sheets throw on `cssRules`: recorded as blocked, never fatal.
  function collectDeclared() {
    const limit = opts.declaredLimit || 200;
    const rules = [];
    const blocked = [];
    const atRules = [];
    const seen = new Set();
    const els = Array.from(capturedEls);
    let truncated = false;

    function add(bucket, rule) {
      const key = String(rule.href) + '|' + rule.index + '|' + rule.selector;
      if (seen.has(key)) return;
      seen.add(key);
      if (bucket.length >= limit) { truncated = true; return; }
      bucket.push(rule);
    }
    function order(list) {
      list.sort(function (a, b) {
        const ha = a.href || '', hb = b.href || '';
        if (ha < hb) return -1;
        if (ha > hb) return 1;
        if (a.index !== b.index) return a.index - b.index;
        if (a.selector < b.selector) return -1;
        if (a.selector > b.selector) return 1;
        return 0;
      });
    }
    function ruleObj(rule, sheet, index, origin) {
      let href = null;
      try { href = strOrNull(sheet.href); } catch (e) {}
      return {
        selector: String(rule.selectorText || ''),
        href: href,
        index: index,
        origin: origin,
        declarations: styleObj(rule.style)
      };
    }
    function walk(list, sheet, bucket) {
      if (!list) return;
      for (let i = 0; i < list.length; i++) {
        const rule = list[i];
        if (rule.cssRules && rule.selectorText === undefined) {
          const inner = [];
          try { walk(rule.cssRules, sheet, inner); } catch (e) {}
          order(inner);
          atRules.push({ condition_text: atRuleText(rule), index: i, rules: inner });
          continue;
        }
        if (rule.selectorText === undefined) continue;
        const selector = String(rule.selectorText);
        let matched = false;
        for (let e = 0; e < els.length; e++) {
          if (ruleMatches(els[e], selector)) { matched = true; break; }
        }
        if (matched) add(bucket, ruleObj(rule, sheet, i, 'author'));
      }
    }

    try {
      for (let s = 0; s < document.styleSheets.length; s++) {
        const sheet = document.styleSheets[s];
        let list;
        try { list = sheet.cssRules; } catch (e) {
          let href = null;
          try { href = strOrNull(sheet.href); } catch (e2) {}
          blocked.push(href || 'inline');
          continue;
        }
        walk(list, sheet, rules);
      }
    } catch (e) {}

    for (let e = 0; e < els.length; e++) {
      const el = els[e];
      if (el.style && el.style.length) {
        add(rules, {
          selector: selectorOf(el),
          href: null,
          index: 0,
          origin: 'inline',
          declarations: styleObj(el.style)
        });
      }
    }
    order(rules);
    return {
      rules: rules,
      blocked_sheets: blocked,
      at_rules: atRules,
      truncated: truncated
    };
  }

