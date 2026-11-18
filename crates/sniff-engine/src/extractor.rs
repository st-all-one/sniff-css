//! Computed-style extraction via a single in-page JavaScript pass.

use crate::{declared, document, env};
use serde_json::{Map, Value, json};
use sniff_cdp::session::CdpSession;
use sniff_core::properties::StyleCategory;
use sniff_core::types::{
    AccessibilityGrade, AriaInfo, ComputedProperty, ComputedStyles, ElementMetrics,
    ElementSnapshot, Noticeability, PseudoStyles, Rect,
};
use sniff_core::{SniffConfig, SniffError, SniffResult};

/// Performs element matching, filtering, style extraction and DOM-tree
/// walk in one `Runtime.evaluate` call to minimize CDP round trips.
const EXTRACT_JS: &str = r#"
(args) => {
  const selector = args.selector;
  const maxDepth = args.depth;
  const categories = args.categories;
  const pseudo = args.pseudo || [];
  const filter = args.filter;
  const opts = args.opts;
  const stableKey = opts.stableKey || null;

  function hexColor(tok) {
    const m = /^rgba?\(([^)]+)\)$/.exec(tok.trim());
    if (!m) return tok;
    const parts = m[1].split(/[,\s/]+/).filter(Boolean);
    if (parts.length < 3) return tok;
    const r = parseInt(parts[0], 10);
    const g = parseInt(parts[1], 10);
    const b = parseInt(parts[2], 10);
    let a;
    if (parts[3] !== undefined) {
      a = parts[3].indexOf('%') >= 0 ? parseFloat(parts[3]) / 100 : parseFloat(parts[3]);
    }
    const h = (n) => n.toString(16).padStart(2, '0');
    if (a === undefined || a >= 1) return '#' + h(r) + h(g) + h(b);
    return '#' + h(r) + h(g) + h(b) + h(Math.round(a * 255));
  }
  function norm(value) {
    if (typeof value !== 'string' || value.indexOf('rgb') < 0) return value;
    return value.replace(/rgba?\([^)]*\)/g, hexColor);
  }

  function classesOf(el) {
    const cls = el.classList;
    return cls ? Array.prototype.slice.call(cls) : [];
  }
  function escAttr(v) {
    return String(v).replace(/\\/g, '\\\\').replace(/"/g, '\\"');
  }
  function anchorOf(el) {
    if (el.id) return '#' + el.id;
    const sk = opts.stableKey;
    if (sk) {
      const v = el.getAttribute(sk);
      if (v) return '[' + sk + '="' + escAttr(v) + '"]';
    }
    return null;
  }
  function token(el) {
    const a = anchorOf(el);
    if (a) return el.tagName.toLowerCase() + a;
    const cls = classesOf(el);
    if (cls.length) return el.tagName.toLowerCase() + '.' + cls[0];
    return el.tagName.toLowerCase();
  }
  function hasAnchor(el) {
    return anchorOf(el) !== null;
  }
  function pathOf(el) {
    const parts = [];
    let cur = el;
    while (cur && cur.nodeType === 1) {
      parts.unshift(token(cur));
      if (hasAnchor(cur)) break;
      cur = cur.parentElement;
    }
    return parts.join(' > ');
  }
  function selectorOf(el) {
    const parts = [];
    let cur = el;
    let n = 0;
    while (cur && cur.nodeType === 1 && n < 32) {
      let tok = token(cur);
      if (hasAnchor(cur)) {
        parts.unshift(tok);
        break;
      }
      const parent = cur.parentElement;
      const siblings = parent ? Array.prototype.slice.call(parent.children) : [];
      if (siblings.length > 1) {
        tok += ':nth-child(' + (siblings.indexOf(cur) + 1) + ')';
      }
      parts.unshift(tok);
      cur = parent;
      n += 1;
    }
    return parts.join(' > ');
  }

  function passes(el) {
    if (filter.visible) {
      const cs = getComputedStyle(el);
      if (cs.display === 'none' || cs.visibility === 'hidden') return false;
      const r = el.getBoundingClientRect();
      if (r.width === 0 && r.height === 0) return false;
    }
    if (filter.minWidth != null || filter.minHeight != null) {
      const r = el.getBoundingClientRect();
      if (filter.minWidth != null && r.width < filter.minWidth) return false;
      if (filter.minHeight != null && r.height < filter.minHeight) return false;
    }
    if (filter.excludeSelectors && filter.excludeSelectors.length) {
      for (let i = 0; i < filter.excludeSelectors.length; i++) {
        if (el.matches(filter.excludeSelectors[i])) return false;
      }
    }
    return true;
  }

  function stackingContext(cs) {
    if (cs.zIndex !== 'auto' && cs.position !== 'static') return true;
    if (cs.position === 'fixed' || cs.position === 'sticky') return true;
    if (parseFloat(cs.opacity) < 1) return true;
    if (cs.transform !== 'none') return true;
    if (cs.perspective !== 'none') return true;
    if (cs.filter !== 'none') return true;
    if (cs.backdropFilter !== 'none') return true;
    if (cs.mixBlendMode !== 'normal') return true;
    if (cs.isolation === 'isolate') return true;
    if (cs.willChange && /(transform|opacity|filter|perspective)/.test(cs.willChange)) return true;
    if (cs.contain && /(layout|paint|strict|content)/.test(cs.contain)) return true;
    if (cs.clipPath !== 'none') return true;
    if (cs.maskImage !== 'none') return true;
    if (cs.contentVisibility === 'auto') return true;
    return false;
  }

  function implicitRole(el) {
    const t = el.tagName;
    switch (t) {
      case 'A': case 'AREA': return el.hasAttribute('href') ? 'link' : null;
      case 'BUTTON': return 'button';
      case 'INPUT': {
        const ty = (el.getAttribute('type') || 'text').toLowerCase();
        if (ty === 'button' || ty === 'submit' || ty === 'reset' || ty === 'image') return 'button';
        if (ty === 'checkbox') return 'checkbox';
        if (ty === 'radio') return 'radio';
        if (ty === 'range') return 'slider';
        if (ty === 'number') return 'spinbutton';
        if (ty === 'search') return 'searchbox';
        if (ty === 'email' || ty === 'url' || ty === 'tel') return 'textbox';
        // file/color/date/time/week/month/datetime-local/password/hidden -> no role
        return null;
      }
      case 'SELECT':
        return (el.hasAttribute('multiple') || parseInt(el.getAttribute('size') || '0', 10) > 1)
          ? 'listbox' : 'combobox';
      case 'OPTION': return 'option';
      case 'OPTGROUP': return 'group';
      case 'TEXTAREA': return 'textbox';
      case 'HEADER': case 'FOOTER':
        // banner/contentinfo only outside article/aside/main/nav/section.
        return el.closest('article,aside,main,nav,section') ? null
          : (t === 'HEADER' ? 'banner' : 'contentinfo');
      case 'NAV': return 'navigation';
      case 'MAIN': return 'main';
      case 'ASIDE': return 'complementary';
      case 'FORM': return 'form';
      case 'SEARCH': return 'search';
      case 'ARTICLE': return 'article';
      case 'SECTION':
        return (el.getAttribute('aria-label') || el.getAttribute('aria-labelledby') || el.getAttribute('title'))
          ? 'region' : null;
      case 'UL': case 'OL': case 'MENU': return 'list';
      case 'LI': return 'listitem';
      case 'DL': return 'list';
      case 'DT': return 'term';
      case 'DD': return 'definition';
      case 'DFN': return 'term';
      case 'TABLE': return 'table';
      case 'CAPTION': return null;
      case 'COLGROUP': case 'THEAD': case 'TBODY': case 'TFOOT': return 'rowgroup';
      case 'COL': return 'column';
      case 'TR': return 'row';
      case 'TD': return 'cell';
      case 'TH': return el.getAttribute('scope') === 'row' ? 'rowheader' : 'columnheader';
      case 'IMG':
        return el.hasAttribute('alt') && el.getAttribute('alt') === '' ? 'presentation' : 'img';
      case 'H1': case 'H2': case 'H3': case 'H4': case 'H5': case 'H6': return 'heading';
      case 'HGROUP': return 'group';
      case 'FIGURE': return 'figure';
      case 'FIGCAPTION': return null;
      case 'FIELDSET': return 'group';
      case 'LEGEND': return null;
      case 'DIALOG': return 'dialog';
      case 'SUMMARY': return 'button';
      case 'DETAILS': return 'group';
      case 'HR': return 'separator';
      case 'PROGRESS': return 'progressbar';
      case 'METER': return 'meter';
      case 'OUTPUT': return 'status';
      case 'MARK': return 'mark';
      case 'STRONG': return 'strong';
      case 'EM': return 'emphasis';
      case 'SUB': return 'subscript';
      case 'SUP': return 'superscript';
      case 'CODE': return 'code';
      case 'BLOCKQUOTE': return 'blockquote';
      case 'TIME': return 'time';
      case 'P': return 'paragraph';
      case 'MATH': return 'math';
      case 'CANVAS': return (el.textContent || '').trim() ? 'img' : null;
      case 'IFRAME': return (el.getAttribute('title') || '').trim() ? 'document' : null;
      case 'EMBED': return 'embedded';
      case 'AUDIO': case 'VIDEO': return el.hasAttribute('controls') ? 'group' : null;
      default: return null;
    }
  }

  function textOf(el) {
    const t = (el.innerText || el.textContent || '').replace(/\s+/g, ' ').trim();
    return t.slice(0, 200);
  }

  function accessibleName(el) {
    const lb = el.getAttribute('aria-labelledby');
    if (lb) {
      const parts = lb.split(/\s+/)
        .map((id) => { const n = document.getElementById(id); return n ? textOf(n) : ''; })
        .filter(Boolean).join(' ');
      if (parts) return parts;
    }
    const label = el.getAttribute('aria-label');
    if (label && label.trim()) return label.trim();
    const alt = el.getAttribute('alt');
    if (alt !== null && (el.tagName === 'IMG' || el.tagName === 'AREA' ||
        (el.tagName === 'INPUT' && el.getAttribute('type') === 'image'))) return alt;
    const title = el.getAttribute('title');
    if (title) return title;
    if (el.tagName === 'INPUT' && el.getAttribute('type') === 'submit') return el.value || '';
    const ph = el.getAttribute('placeholder');
    if (ph) return ph;
    const role = el.getAttribute('role') || implicitRole(el) || '';
    if (['button', 'link', 'heading', 'tab', 'menuitem', 'checkbox', 'radio', 'option',
         'searchbox', 'textbox', 'combobox'].indexOf(role) >= 0) {
      const t = textOf(el);
      if (t) return t;
    }
    return '';
  }

  function isFocusable(el) {
    if (el.disabled === true) return false;
    const ti = el.getAttribute('tabindex');
    if (ti !== null && ti !== '') {
      const n = parseInt(ti, 10);
      if (!isNaN(n) && n >= 0) return true;
    }
    if (el.hasAttribute('hidden')) return false;
    const t = el.tagName;
    if (t === 'A' || t === 'AREA') return el.hasAttribute('href');
    if (t === 'BUTTON' || t === 'SELECT' || t === 'TEXTAREA' || t === 'SUMMARY' || t === 'IFRAME') return true;
    if (t === 'INPUT') return (el.getAttribute('type') || 'text') !== 'hidden';
    if (t === 'DETAILS') return true;
    return false;
  }

  function ariaOf(el) {
    const out = {};
    const role = el.getAttribute('role');
    const implicit = implicitRole(el);
    if (role) out.role = role; else if (implicit) out.role = implicit;
    const name = accessibleName(el);
    if (name) out.name = name;
    out.focusable = isFocusable(el);
    const attrs = [
      ['aria-hidden', 'ariaHidden'], ['aria-expanded', 'ariaExpanded'],
      ['aria-checked', 'ariaChecked'], ['aria-controls', 'ariaControls'],
      ['aria-labelledby', 'ariaLabelledby'], ['aria-describedby', 'ariaDescribedby'],
      ['lang', 'lang'], ['alt', 'alt'], ['title', 'title']
    ];
    for (let i = 0; i < attrs.length; i++) {
      const v = el.getAttribute(attrs[i][0]);
      if (v !== null && v !== '') out[attrs[i][1]] = v;
    }
    out.htmlHidden = el.hasAttribute('hidden');
    out.disabled = el.disabled === true || el.getAttribute('aria-disabled') === 'true';
    out.hasText = (() => {
      const nodes = el.childNodes;
      for (let i = 0; i < nodes.length; i++) {
        if (nodes[i].nodeType === 3 && (nodes[i].textContent || '').trim()) return true;
      }
      return false;
    })();
    return out;
  }

  function buildStyles(el, cs) {
    const result = {};
    const keys = Object.keys(categories);
    for (let ci = 0; ci < keys.length; ci++) {
      const key = keys[ci];
      const props = categories[key];
      const values = {};
      for (let pi = 0; pi < props.length; pi++) {
        const name = props[pi];
        let v = cs.getPropertyValue(name);
        if (opts.normalizeColors) v = norm(v);
        values[name] = v;
      }
      result[key] = values;
    }
    if (opts.customProps) {
      const values = {};
      for (let i = 0; i < cs.length; i++) {
        const name = cs[i];
        if (name.indexOf('--') === 0) {
          values[name] = cs.getPropertyValue(name);
        }
      }
      result['css_variables'] = values;
    }
    return result;
  }

  function buildPseudo(el) {
    const out = {};
    for (let i = 0; i < pseudo.length; i++) {
      const pname = pseudo[i];
      const cs = getComputedStyle(el, pname);
      const groups = {};
      const keys = Object.keys(categories);
      for (let ci = 0; ci < keys.length; ci++) {
        const props = categories[keys[ci]];
        const values = {};
        for (let pi = 0; pi < props.length; pi++) {
          const name = props[pi];
          let v = cs.getPropertyValue(name);
          if (opts.normalizeColors) v = norm(v);
          values[name] = v;
        }
        groups[keys[ci]] = values;
      }
      if (opts.customProps) {
        const values = {};
        for (let i = 0; i < cs.length; i++) {
          const name = cs[i];
          if (name.indexOf('--') === 0) {
            values[name] = cs.getPropertyValue(name);
          }
        }
        groups['css_variables'] = values;
      }
      out[pname] = groups;
    }
    return out;
  }

  function parseRgba(color) {
    if (!color) return null;
    const m = /^rgba?\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)(\s*[,/]\s*([\d.]+%?))?\s*\)$/.exec(color);
    if (!m) return null;
    let a = 1;
    if (m[5] !== undefined) a = m[5].endsWith('%') ? parseFloat(m[5]) / 100 : parseFloat(m[5]);
    return [+m[1], +m[2], +m[3], a];
  }

  // Effective background painted BEHIND `node` (its own bg composited over
  // every ancestor up to the html/body canvas), exactly as the browser
  // renders it. Returns '#rrggbb' for a solid effective color, 'image' when
  // any layer in the chain paints a background image, or null when nothing
  // opaque is resolvable. The JS climb is independent of the capture depth,
  // so a transparent capture root still resolves to the real page color.
  function effectiveBackground(node) {
    if (!node) {
      // Above <html>: the canvas. Body/html backgrounds propagate to it.
      const html = parseRgba(getComputedStyle(document.documentElement).backgroundColor);
      const body = parseRgba(getComputedStyle(document.body).backgroundColor);
      const solid = [html, body].find(function (x) { return x && x[3] >= 1; });
      return solid ? toHexRgb(solid[0], solid[1], solid[2]) : null;
    }
    const cs = getComputedStyle(node);
    if (cs.backgroundImage && cs.backgroundImage !== 'none') return 'image';
    const bg = parseRgba(cs.backgroundColor);
    if (bg) {
      const r = bg[0], g = bg[1], b = bg[2], a = bg[3];
      if (a >= 1) return toHexRgb(r, g, b);
      if (a > 0) {
        const below = effectiveBackground(node.parentElement);
        if (below === 'image') return 'image';
        if (!below) return null;
        const [br, bg_, bb] = fromHexRgb(below);
        return toHexRgb(
          r * a + br * (1 - a),
          g * a + bg_ * (1 - a),
          b * a + bb * (1 - a)
        );
      }
    }
    return effectiveBackground(node.parentElement);
  }

  function toHexRgb(r, g, b) {
    const h = function (v) {
      const c = Math.round(v);
      return (c < 0 ? 0 : c > 255 ? 255 : c).toString(16).padStart(2, '0');
    };
    return '#' + h(r) + h(g) + h(b);
  }

  function fromHexRgb(hex) {
    return [
      parseInt(hex.slice(1, 3), 16),
      parseInt(hex.slice(3, 5), 16),
      parseInt(hex.slice(5, 7), 16)
    ];
  }

  function accessibilityGradeOf(el, cs, rect, aria) {
    // Exposed to assistive tech at all?
    const ariaHidden = aria && aria.ariaHidden === 'true';
    const htmlHidden = el.hasAttribute('hidden') || !!el.closest('[hidden],[inert]');
    const displayNone = cs.display === 'none';
    const visibilityHidden = cs.visibility === 'hidden' || cs.visibility === 'collapse';
    const zeroSize = rect.width === 0 && rect.height === 0;
    if (ariaHidden || htmlHidden || displayNone || visibilityHidden || zeroSize) {
      return 'NONE';
    }
    // On screen within the current viewport?
    const onScreen = rect.x + rect.width > 0 && rect.y + rect.height > 0 &&
      rect.x < window.innerWidth && rect.y < window.innerHeight;
    const opacityZero = parseFloat(cs.opacity) === 0;
    // Roles that require an accessible name.
    const role = (aria && aria.role) || '';
    const needsName = ['button','link','img','heading','checkbox','radio','switch',
      'textbox','searchbox','combobox','slider','spinbutton','option','tab','menuitem',
      'progressbar','meter','status','dialog','navigation','main','banner','contentinfo',
      'complementary','region','search','form','article','figure'].indexOf(role) >= 0;
    const hasName = aria && aria.name && aria.name.trim().length > 0;
    if (!onScreen || opacityZero || (needsName && !hasName)) {
      return 'AA';
    }
    return 'AAA';
  }

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

  const capturedEls = new Set();
  let __shadowRoots = 0;

  function buildNode(el, depth, parentId, inShadow) {
    const node = {};
    node.id = ++__nodeId;
    if (inShadow) node.shadow = true;
    capturedEls.add(el);
    if (parentId) node.parentId = parentId;
    node.tag = el.tagName;
    node.selector = (inShadow ? '#shadow-root > ' : '') + selectorOf(el);
    node.path = (inShadow ? '#shadow-root > ' : '') + pathOf(el);
    node.depth = depth;
    const cs = getComputedStyle(el);
    let rect = null;
    if (opts.includeRect || opts.includeVisibility) {
      rect = el.getBoundingClientRect();
    }
    if (opts.includeRect) {
      node.rect = { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
    }
    if (opts.includeMetrics) {
      node.metrics = { z_index: cs.zIndex, stacking_context: stackingContext(cs) };
    }
    if (opts.includeAria) {
      node.aria = ariaOf(el);
    }
    if (opts.includeVisibility) {
      const rendered = cs.display !== 'none' &&
        (cs.visibility !== 'hidden' && cs.visibility !== 'collapse') &&
        parseFloat(cs.opacity) > 0 && (rect.width > 0 || rect.height > 0);
      node.isUserNoticeable = {
        display_visible: rendered,
        accessibility_grade: accessibilityGradeOf(el, cs, rect, node.aria)
      };
    }
    if (opts.includeContrast) {
      node.effectiveBackground = effectiveBackground(el);
    }
    if (opts.allAttributes) {
      const attrs = {};
      let names = el.getAttributeNames();
      for (let i = 0; i < names.length; i++) attrs[names[i]] = el.getAttribute(names[i]);
      if (names.length) node.attributes = attrs;
    } else if (opts.attributes && opts.attributes.length) {
      const attrs = {};
      let any = false;
      for (let i = 0; i < opts.attributes.length; i++) {
        const v = el.getAttribute(opts.attributes[i]);
        if (v !== null) { attrs[opts.attributes[i]] = v; any = true; }
      }
      if (any) node.attributes = attrs;
    }
    node.styles = buildStyles(el, cs);
    if (pseudo.length) {
      node.pseudo = buildPseudo(el);
    }
    if (depth < maxDepth) {
      const kids = Array.prototype.slice.call(el.children);
      const children = [];
      for (let i = 0; i < kids.length; i++) {
        if (!passes(kids[i])) continue;
        children.push(buildNode(kids[i], depth + 1, node.id));
      }
      node.children = children;
    } else {
      node.children = [];
    }
    return node;
  }

  const results = [];
  let __nodeId = 0;
  const roots = document.querySelectorAll(selector);
  for (let i = 0; i < roots.length; i++) {
    if (!passes(roots[i])) continue;
    results.push(buildNode(roots[i], 0, null));
  }

  let globalCssVars = null;
  if (opts.customProps) {
    const rootCs = getComputedStyle(document.documentElement);
    const g = {};
    for (let i = 0; i < rootCs.length; i++) {
      const n = rootCs[i];
      if (n.indexOf('--') === 0) g[n] = rootCs.getPropertyValue(n);
    }
    globalCssVars = g;
  }
  const documentFacet = opts.documentFacet ? collectDocumentFacet() : null;
  const envFacet = opts.envFacet ? collectEnv() : null;
  const declaredFacet = opts.declaredFacet ? collectDeclared() : null;
  return {
    globalCssVars: globalCssVars,
    elements: results,
    documentFacet: documentFacet,
    envFacet: envFacet,
    declaredFacet: declaredFacet,
    shadowRoots: __shadowRoots
  };
}
"#;

/// Result of an extraction run: snapshots plus, when custom properties
/// were captured, the global `:root` variable map (once, not per node).
#[derive(Debug, Clone, Default)]
pub struct SniffOutcome {
    /// The extracted snapshot forest.
    pub snapshots: Vec<ElementSnapshot>,
    /// Global `:root` CSS custom properties, captured once when requested.
    pub global_css_variables: Option<Vec<(String, String)>>,
    /// Global `__document` facet (title/lang/meta/links/scripts/sheets).
    pub document_facet: Option<sniff_core::modern::DocumentFacet>,
    /// Global `__env` facet (normalized media prefs, emulation, device).
    pub env_facet: Option<sniff_core::modern::EnvFacet>,
    /// Global `__declared` facet (CSSOM rules, opt-in via `--declared`).
    pub declared_facet: Option<sniff_core::modern::DeclaredFacet>,
    /// Matrix state tags when more than one viewport/emulation was captured.
    pub matrix: Vec<sniff_core::modern::MatrixTag>,
    /// Open shadow roots traversed during extraction.
    pub shadow_roots: usize,
    /// Full accessibility subtree (CDP `Accessibility` domain) for the
    /// matched elements, when `ax_tree` capture was requested.
    pub ax_tree: Option<Value>,
    /// UI-effect map for the configured interaction actions (the reserved
    /// `__actions` output area): per action, what appeared/disappeared/
    /// changed and where. `None` when there were no actions or `effects`
    /// is disabled.
    pub actions: Option<Value>,
    /// Decoded PNG bytes of the page when `config.screenshot` is set.
    pub screenshot: Option<Vec<u8>>,
}

/// Run the extraction pass and convert the returned JSON to snapshots.
pub async fn extract(session: &CdpSession, config: &SniffConfig) -> SniffResult<SniffOutcome> {
    let args = build_args(config);
    let args_json = serde_json::to_string(&args).map_err(SniffError::from)?;
    let expression = format!("({EXTRACT_JS})\n({args_json})");

    let value = session
        .evaluate(&expression, false)
        .await
        .map_err(|e| SniffError::Cdp(e.to_string()))?;

    parse_results(&value, config)
}

/// Build the argument object consumed by the in-page script.
///
/// NOTE: keys mirror the camelCase field names read by `EXTRACT_JS`.
fn build_args(config: &SniffConfig) -> Value {
    let categories = build_categories(config);
    let filter = json!({
        "visible": config.filter.visible,
        "minWidth": config.filter.min_width,
        "minHeight": config.filter.min_height,
        "excludeSelectors": config.filter.exclude_selectors,
    });
    let opts = json!({
        "includeRect": config.output.include_rect,
        "includeMetrics": config.output.include_metrics,
        "includeVisibility": config.output.include_visibility,
        "includeAria": config.output.include_aria,
        "includeContrast": config.output.include_contrast,
        "normalizeColors": config.output.normalize_colors,
        "customProps": config.include_custom_properties,
        "stableKey": config.stable_key,
        "attributes": config.effective_attributes(),
        "allAttributes": config.all_attributes(),
        "documentFacet": config.document_enabled(),
        "envFacet": config.env_enabled(),
        "declaredFacet": config.declared_facet,
        "declaredLimit": config.declared_limit,
        "emulate": config.emulate,
    });
    json!({
        "selector": config.selector,
        "depth": config.depth,
        "categories": categories,
        "pseudo": config.pseudo_elements,
        "filter": filter,
        "opts": opts,
    })
}

/// Resolve categories (including `All` and custom properties) into the
/// `{ category_key: [props] }` map consumed by the page.
fn build_categories(config: &SniffConfig) -> Map<String, Value> {
    let mut map = Map::new();
    let has_all = config.categories.contains(&StyleCategory::All);
    for cat in &config.categories {
        if has_all || *cat == StyleCategory::Custom {
            continue;
        }
        map.insert(
            cat.key().to_string(),
            Value::Array(
                cat.properties()
                    .iter()
                    .map(|p| Value::String(p.to_string()))
                    .collect(),
            ),
        );
    }
    if has_all {
        for cat in StyleCategory::all() {
            map.insert(
                cat.key().to_string(),
                Value::Array(
                    cat.properties()
                        .iter()
                        .map(|p| Value::String(p.to_string()))
                        .collect(),
                ),
            );
        }
    }
    if !config.custom_properties.is_empty() {
        map.insert(
            "custom".to_string(),
            Value::Array(
                config
                    .custom_properties
                    .iter()
                    .map(|s| Value::String(s.clone()))
                    .collect(),
            ),
        );
    }
    map
}

/// Map a JSON category key back to its `StyleCategory`.
fn category_from_key(key: &str) -> StyleCategory {
    for cat in StyleCategory::all() {
        if cat.key() == key {
            return cat;
        }
    }
    if key == "custom" {
        return StyleCategory::Custom;
    }
    if key == "css_variables" {
        return StyleCategory::Variables;
    }
    StyleCategory::All
}

/// Parse the `{ globalCssVars, elements, ...facets }` object returned by the page.
fn parse_results(value: &Value, _config: &SniffConfig) -> SniffResult<SniffOutcome> {
    let elements = value
        .get("elements")
        .and_then(Value::as_array)
        .ok_or_else(|| SniffError::Other("extraction returned no `elements` array".into()))?;
    let snapshots = elements.iter().map(parse_node).collect();

    let global_css_variables = value
        .get("globalCssVars")
        .and_then(Value::as_object)
        .map(|obj| {
            obj.iter()
                .map(|(name, val)| (name.clone(), val.as_str().unwrap_or_default().to_string()))
                .collect()
        })
        .filter(|v: &Vec<(String, String)>| !v.is_empty());

    let parse_facet = |key: &str| value.get(key).filter(|v| !v.is_null());
    Ok(SniffOutcome {
        snapshots,
        global_css_variables,
        document_facet: parse_facet("documentFacet").map(document::parse),
        env_facet: parse_facet("envFacet").map(env::parse),
        declared_facet: parse_facet("declaredFacet").map(declared::parse),
        shadow_roots: value
            .get("shadowRoots")
            .and_then(Value::as_u64)
            .map_or(0, |n| usize::try_from(n).unwrap_or(0)),
        ..SniffOutcome::default()
    })
}

fn parse_node(v: &Value) -> ElementSnapshot {
    ElementSnapshot {
        id: v.get("id").and_then(Value::as_u64).unwrap_or(0),
        parent_id: v.get("parentId").and_then(Value::as_u64),
        tag: v
            .get("tag")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        selector: v
            .get("selector")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        path: v
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        depth: v
            .get("depth")
            .and_then(Value::as_u64)
            .map_or(0, |d| usize::try_from(d).unwrap_or(usize::MAX)),
        rect: v.get("rect").map(parse_rect),
        metrics: v.get("metrics").map(parse_metrics),
        noticeable: v.get("isUserNoticeable").and_then(parse_noticeable),
        effective_background: v
            .get("effectiveBackground")
            .and_then(Value::as_str)
            .map(String::from),
        aria: v.get("aria").map(parse_aria),
        contrast: None,
        ax: None,
        attributes: v.get("attributes").and_then(Value::as_object).map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        }),
        styles: parse_styles(v.get("styles").unwrap_or(&Value::Null)),
        pseudo: v
            .get("pseudo")
            .and_then(Value::as_object)
            .map(|obj| {
                obj.iter()
                    .map(|(name, styles)| PseudoStyles {
                        name: name.clone(),
                        styles: parse_styles(styles),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        children: v
            .get("children")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().map(parse_node).collect())
            .unwrap_or_default(),
    }
}

fn parse_aria(v: &Value) -> AriaInfo {
    let str_opt = |key: &str| v.get(key).and_then(Value::as_str).map(String::from);
    let flag = |key: &str| v.get(key).and_then(Value::as_bool).unwrap_or(false);
    AriaInfo {
        role: str_opt("role"),
        name: str_opt("name"),
        focusable: flag("focusable"),
        aria_hidden: str_opt("ariaHidden"),
        aria_expanded: str_opt("ariaExpanded"),
        aria_checked: str_opt("ariaChecked"),
        aria_controls: str_opt("ariaControls"),
        aria_labelledby: str_opt("ariaLabelledby"),
        aria_describedby: str_opt("ariaDescribedby"),
        lang: str_opt("lang"),
        alt: str_opt("alt"),
        title: str_opt("title"),
        html_hidden: flag("htmlHidden"),
        disabled: flag("disabled"),
        has_text: flag("hasText"),
    }
}

fn parse_rect(v: &Value) -> Rect {
    Rect {
        x: v.get("x").and_then(Value::as_f64).unwrap_or(0.0),
        y: v.get("y").and_then(Value::as_f64).unwrap_or(0.0),
        width: v.get("width").and_then(Value::as_f64).unwrap_or(0.0),
        height: v.get("height").and_then(Value::as_f64).unwrap_or(0.0),
    }
}

fn parse_metrics(v: &Value) -> ElementMetrics {
    ElementMetrics {
        z_index: v
            .get("z_index")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        stacking_context: v
            .get("stacking_context")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

fn parse_noticeable(v: &Value) -> Option<Noticeability> {
    let display_visible = v.get("display_visible").and_then(Value::as_bool)?;
    let grade = match v
        .get("accessibility_grade")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_uppercase()
        .as_str()
    {
        "NONE" => AccessibilityGrade::None,
        "AA" => AccessibilityGrade::Aa,
        "AAA" => AccessibilityGrade::Aaa,
        _ => return None,
    };
    Some(Noticeability {
        display_visible,
        accessibility_grade: grade,
    })
}

fn parse_styles(v: &Value) -> ComputedStyles {
    let Some(obj) = v.as_object() else {
        return ComputedStyles::default();
    };
    let groups = obj
        .iter()
        .map(|(key, props)| {
            let props = props
                .as_object()
                .map(|p| {
                    p.iter()
                        .map(|(name, value)| ComputedProperty {
                            name: name.clone(),
                            value: value.as_str().unwrap_or_default().to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            (category_from_key(key), props)
        })
        .collect();
    ComputedStyles { groups }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_key_round_trip() {
        assert_eq!(category_from_key("box_model"), StyleCategory::BoxModel);
        assert_eq!(category_from_key("custom"), StyleCategory::Custom);
        assert_eq!(category_from_key("css_variables"), StyleCategory::Variables);
        assert_eq!(category_from_key("unknown"), StyleCategory::All);
    }

    #[test]
    fn build_categories_resolves_all() {
        let cfg = SniffConfig {
            categories: vec![StyleCategory::All],
            custom_properties: vec!["--my-prop".into()],
            ..Default::default()
        };
        let map = build_categories(&cfg);
        assert!(map.contains_key("box_model"));
        assert!(map.contains_key("animation"));
        assert_eq!(map.get("custom").unwrap().as_array().unwrap().len(), 1);
        assert_eq!(
            map.get("box_model").unwrap().as_array().unwrap()[0],
            Value::String("width".into())
        );
    }

    #[test]
    fn build_categories_honors_selection_and_custom() {
        let cfg = SniffConfig {
            categories: vec![StyleCategory::Typography, StyleCategory::Custom],
            custom_properties: vec!["--x".into()],
            ..Default::default()
        };
        let map = build_categories(&cfg);
        assert!(map.contains_key("typography"));
        assert!(!map.contains_key("box_model"));
        assert!(map.contains_key("custom"));
    }

    #[test]
    fn parse_node_full_tree() {
        let json = json!({
            "tag": "DIV",
            "selector": "div.a",
            "path": "body > div.a",
            "depth": 0,
            "rect": {"x": 1.0, "y": 2.0, "width": 100.0, "height": 50.0},
            "metrics": {"z_index": "10", "stacking_context": true},
            "aria": {"role": "link", "name": "Go", "focusable": true, "ariaHidden": "false"},
            "styles": {
                "box_model": {"width": "100px"},
                "layout": {"display": "block"}
            },
            "children": [
                {"tag": "SPAN", "selector": "div.a > span", "path": "body > div.a > span",
                 "depth": 1, "styles": {"layout": {"display": "inline"}}, "children": []}
            ]
        });
        let snap = parse_node(&json);
        assert_eq!(snap.tag, "DIV");
        assert_eq!(snap.rect.unwrap().width, 100.0);
        assert_eq!(snap.metrics.unwrap().z_index, "10");
        assert_eq!(snap.styles.groups.len(), 2);
        assert_eq!(snap.children.len(), 1);
        assert_eq!(snap.children[0].depth, 1);
        let aria = snap.aria.expect("aria parsed");
        assert_eq!(aria.role.as_deref(), Some("link"));
        assert_eq!(aria.name.as_deref(), Some("Go"));
        assert!(aria.focusable);
        assert_eq!(aria.aria_hidden.as_deref(), Some("false"));
        assert_eq!(snap.contrast, None);
        assert_eq!(snap.ax, None);
    }

    #[test]
    fn parse_aria_missing_fields_default() {
        let snap = parse_node(&json!({
            "tag": "DIV", "selector": "div",
            "aria": {"focusable": false, "htmlHidden": false, "disabled": false},
            "styles": {}, "children": []
        }));
        let aria = snap.aria.expect("aria facet parsed");
        assert_eq!(aria.role, None);
        assert!(!aria.focusable);
        assert!(!aria.html_hidden);
        assert!(!aria.disabled);
    }

    #[test]
    fn parse_styles_empty_is_default() {
        let styles = parse_styles(&Value::Null);
        assert!(styles.is_empty());
    }

    #[test]
    fn build_args_keys_match_js_reads() {
        let cfg = SniffConfig {
            url: "http://x".into(),
            selector: ".card".into(),
            depth: 2,
            categories: vec![StyleCategory::BoxModel],
            custom_properties: vec!["--x".into()],
            pseudo_elements: vec!["::before".into()],
            filter: sniff_core::ElementFilter {
                visible: false,
                min_width: Some(10.0),
                min_height: Some(20.0),
                exclude_selectors: vec![".skip".into()],
            },
            ..Default::default()
        };
        let args = build_args(&cfg);
        // Keys the JS reads must be present with camelCase spelling.
        let obj = args.as_object().unwrap();
        assert_eq!(obj["selector"], ".card");
        assert_eq!(obj["depth"], 2);
        assert_eq!(obj["pseudo"][0], "::before");
        let filter = obj["filter"].as_object().unwrap();
        assert_eq!(filter["visible"], false);
        assert_eq!(filter["minWidth"], 10.0);
        assert_eq!(filter["minHeight"], 20.0);
        assert_eq!(filter["excludeSelectors"][0], ".skip");
        let opts = obj["opts"].as_object().unwrap();
        assert!(opts.contains_key("includeRect"));
        assert!(opts.contains_key("includeMetrics"));
        assert!(opts.contains_key("normalizeColors"));
        assert_eq!(opts["includeVisibility"], true);
        assert_eq!(opts["stableKey"], Value::Null);
        // JS must reference the same spellings (guards against drift).
        let js = EXTRACT_JS.to_string();
        assert!(js.contains("opts.includeRect"));
        assert!(js.contains("opts.includeMetrics"));
        assert!(js.contains("opts.normalizeColors"));
        assert!(js.contains("opts.includeVisibility"));
        assert!(js.contains("opts.stableKey"));
        assert!(js.contains("opts.includeAria"));
        assert!(js.contains("ariaOf(el)"));
        assert!(js.contains("filter.minWidth"));
        assert!(js.contains("filter.minHeight"));
        assert!(js.contains("filter.excludeSelectors"));
    }

    #[test]
    fn build_args_include_custom_props_flag() {
        let cfg = SniffConfig {
            include_custom_properties: true,
            ..Default::default()
        };
        let args = build_args(&cfg);
        assert_eq!(args["opts"]["customProps"], Value::Bool(true));
        // The JS side must collect `--*` into a `css_variables` group.
        let js = EXTRACT_JS.to_string();
        assert!(js.contains("opts.customProps"));
        assert!(js.contains("'css_variables'"));
    }

    #[test]
    fn build_args_carries_stable_key() {
        let cfg = SniffConfig {
            stable_key: Some("data-testid".into()),
            ..Default::default()
        };
        let args = build_args(&cfg);
        assert_eq!(args["opts"]["stableKey"], "data-testid");
        // JS must prefer the stable attribute as the selector anchor.
        let js = EXTRACT_JS.to_string();
        assert!(js.contains("opts.stableKey"));
        assert!(js.contains("el.getAttribute(sk)"));
    }

    #[test]
    fn implicit_role_covers_arria_spec_tags() {
        let js = EXTRACT_JS.to_string();
        // The implicit-role switch must map the HTML-ARIA implicit roles the
        // accessibility guide documents. Guards against drift when the map
        // is enriched.
        let expected: &[(&str, &str)] = &[
            ("A", "case 'A': case 'AREA':"),
            ("BUTTON", "case 'BUTTON': return 'button'"),
            ("INPUT types", "'checkbox'"),
            ("SELECT multiple -> listbox", "'listbox' : 'combobox'"),
            ("OPTION", "case 'OPTION': return 'option'"),
            ("OPTGROUP", "case 'OPTGROUP': return 'group'"),
            (
                "HEADER/FOOTER scoping",
                "el.closest('article,aside,main,nav,section')",
            ),
            ("NAV", "case 'NAV': return 'navigation'"),
            ("MAIN", "case 'MAIN': return 'main'"),
            ("ASIDE", "case 'ASIDE': return 'complementary'"),
            ("FORM", "case 'FORM': return 'form'"),
            ("SEARCH", "case 'SEARCH': return 'search'"),
            ("ARTICLE", "case 'ARTICLE': return 'article'"),
            ("SECTION->region", "'region' : null"),
            (
                "UL/OL/MENU",
                "case 'UL': case 'OL': case 'MENU': return 'list'",
            ),
            ("LI", "case 'LI': return 'listitem'"),
            ("DL", "case 'DL': return 'list'"),
            ("DT->term", "case 'DT': return 'term'"),
            ("DD->definition", "case 'DD': return 'definition'"),
            ("DFN->term", "case 'DFN': return 'term'"),
            ("TABLE", "case 'TABLE': return 'table'"),
            (
                "COLGROUP/THEAD/TBODY/TFOOT->rowgroup",
                "case 'COLGROUP': case 'THEAD': case 'TBODY': case 'TFOOT': return 'rowgroup'",
            ),
            ("COL->column", "case 'COL': return 'column'"),
            ("TR->row", "case 'TR': return 'row'"),
            ("TD->cell", "case 'TD': return 'cell'"),
            (
                "TH scope",
                "el.getAttribute('scope') === 'row' ? 'rowheader' : 'columnheader'",
            ),
            ("IMG alt=\"\"->presentation", "'presentation' : 'img'"),
            (
                "H1-H6->heading",
                "case 'H1': case 'H2': case 'H3': case 'H4': case 'H5': case 'H6': return 'heading'",
            ),
            ("FIGURE->figure", "case 'FIGURE': return 'figure'"),
            ("FIELDSET->group", "case 'FIELDSET': return 'group'"),
            ("DIALOG", "case 'DIALOG': return 'dialog'"),
            ("SUMMARY->button", "case 'SUMMARY': return 'button'"),
            ("DETAILS->group", "case 'DETAILS': return 'group'"),
            ("HR->separator", "case 'HR': return 'separator'"),
            (
                "PROGRESS->progressbar",
                "case 'PROGRESS': return 'progressbar'",
            ),
            ("METER->meter", "case 'METER': return 'meter'"),
            ("OUTPUT->status", "case 'OUTPUT': return 'status'"),
            ("MARK->mark", "case 'MARK': return 'mark'"),
            ("STRONG->strong", "case 'STRONG': return 'strong'"),
            ("EM->emphasis", "case 'EM': return 'emphasis'"),
            ("SUB->subscript", "case 'SUB': return 'subscript'"),
            ("SUP->superscript", "case 'SUP': return 'superscript'"),
            ("CODE->code", "case 'CODE': return 'code'"),
            (
                "BLOCKQUOTE->blockquote",
                "case 'BLOCKQUOTE': return 'blockquote'",
            ),
            ("TIME->time", "case 'TIME': return 'time'"),
            ("P->paragraph", "case 'P': return 'paragraph'"),
            ("MATH->math", "case 'MATH': return 'math'"),
            ("CANVAS fallback", "case 'CANVAS':"),
            ("IFRAME title->document", "case 'IFRAME':"),
            ("EMBED->embedded", "case 'EMBED': return 'embedded'"),
            ("AUDIO/VIDEO controls->group", "case 'AUDIO': case 'VIDEO':"),
        ];
        for (label, needle) in expected {
            assert!(
                js.contains(needle),
                "implicitRole must map {label} (missing `{needle}`)"
            );
        }
    }
}
