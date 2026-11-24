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

