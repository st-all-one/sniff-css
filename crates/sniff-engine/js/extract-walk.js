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
