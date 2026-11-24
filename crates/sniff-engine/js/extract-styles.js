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

