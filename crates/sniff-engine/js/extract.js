(args) => {
  const selector = args.selector;
  const maxDepth = args.depth;
  const categories = args.categories;
  const pseudo = args.pseudo || [];
  const filter = args.filter;
  const opts = args.opts;
  const stableKey = opts.stableKey || null;

