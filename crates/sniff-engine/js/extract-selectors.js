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

