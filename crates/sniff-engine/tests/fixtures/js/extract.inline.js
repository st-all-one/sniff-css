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
