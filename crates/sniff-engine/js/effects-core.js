  function normColor(value) {
    if (typeof value !== 'string' || value.indexOf('rgb') < 0) return value;
    return value.replace(/rgba?\(([^)]*)\)/g, function (m, inner) {
      const parts = inner.split(/[,\s/]+/).filter(Boolean);
      if (parts.length < 3) return m;
      const r = parseInt(parts[0], 10), g = parseInt(parts[1], 10), b = parseInt(parts[2], 10);
      let a;
      if (parts[3] !== undefined) a = parts[3].indexOf('%') >= 0 ? parseFloat(parts[3]) / 100 : parseFloat(parts[3]);
      const h = (v) => v.toString(16).padStart(2, '0');
      if (a === undefined || a >= 1) return '#' + h(r) + h(g) + h(b);
      return '#' + h(r) + h(g) + h(b) + h(Math.round(a * 255));
    });
  }
  function signature(cs) {
    const out = {};
    for (let i = 0; i < SIG.length; i++) {
      const name = SIG[i];
      out[name] = normColor(cs.getPropertyValue(name));
    }
    return out;
  }

  function accessibleName(el) {
    const lb = el.getAttribute('aria-labelledby');
    if (lb) {
      const parts = lb.split(/\s+/)
        .map((id) => { const n = document.getElementById(id); return n ? (n.innerText || '').replace(/\s+/g, ' ').trim().slice(0, 60) : ''; })
        .filter(Boolean);
      if (parts.length) return parts.join(' ');
    }
    const l = el.getAttribute('aria-label');
    if (l && l.trim()) return l.trim();
    const alt = el.getAttribute('alt');
    if (alt !== null && el.tagName === 'IMG') return alt;
    const t = el.getAttribute('title');
    if (t) return t;
    return '';
  }
  function implicitRole(el) {
    switch (el.tagName) {
      case 'A': case 'AREA': return el.hasAttribute('href') ? 'link' : null;
      case 'BUTTON': return 'button';
      case 'INPUT': {
        const ty = (el.getAttribute('type') || 'text').toLowerCase();
        if (ty === 'button' || ty === 'submit' || ty === 'reset' || ty === 'image') return 'button';
        if (ty === 'checkbox') return 'checkbox';
        if (ty === 'radio') return 'radio';
        if (ty === 'range') return 'slider';
        if (ty === 'search') return 'searchbox';
        return 'textbox';
      }
      case 'SELECT': return (el.hasAttribute('multiple') || parseInt(el.getAttribute('size') || '0', 10) > 1) ? 'listbox' : 'combobox';
      case 'OPTION': return 'option';
      case 'TEXTAREA': return 'textbox';
      case 'NAV': return 'navigation';
      case 'MAIN': return 'main';
      case 'IMG': return (el.getAttribute('alt') === '') ? 'presentation' : 'img';
      case 'H1': case 'H2': case 'H3': case 'H4': case 'H5': case 'H6': return 'heading';
      case 'TABLE': return 'table';
      case 'TR': return 'row';
      case 'TD': return 'cell';
      case 'TH': return 'columnheader';
      case 'DIALOG': return 'dialog';
      default: return null;
    }
  }

