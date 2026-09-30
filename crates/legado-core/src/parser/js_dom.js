// Common read-only Jsoup API. Unsupported CSS raises an error, not an empty success.
(function () {
    function select(html, css, base) {
        const result = JSON.parse(__domSelect(String(html), String(css)));
        if (result.error) throw Error(result.error);
        return elements(result.nodes.map(data => element(data, base)));
    }
    function elements(nodes) {
        Object.defineProperties(nodes, {
            get: {value: i => nodes[i]},
            size: {value: () => nodes.length},
            first: {value: () => nodes[0] || null},
            last: {value: () => nodes[nodes.length - 1] || null},
            isEmpty: {value: () => nodes.length === 0},
            text: {value: () => nodes.map(n => n.text()).join(' ')},
            attr: {value: name => nodes.length ? nodes[0].attr(name) : ''},
            html: {value: () => nodes.map(n => n.html()).join('\n')},
            outerHtml: {value: () => nodes.map(n => n.outerHtml()).join('\n')},
            select: {value: css => elements(nodes.flatMap(n => n.select(css)))},
            toArray: {value: () => Array.from(nodes)}
        });
        return nodes;
    }
    function element(data, base) {
        return {
            text: () => data.text,
            html: () => data.inner,
            outerHtml: () => data.outer,
            attr: name => String(name).startsWith('abs:')
                ? __domResolve(base, data.attrs[String(name).slice(4)] || '')
                : data.attrs[String(name)] || '',
            absUrl: name => __domResolve(base, data.attrs[String(name)] || ''),
            hasAttr: name => Object.prototype.hasOwnProperty.call(data.attrs, String(name)),
            select: css => select(data.outer, css, base),
            selectFirst: css => select(data.outer, css, base).first(),
            toString: () => data.outer
        };
    }
    const Jsoup = {parse: (html, base) => {
        const markup = String(html);
        const uri = base == null ? String(globalThis.baseUrl || globalThis.url || '') : String(base);
        return {
            select: css => select(markup, css, uri),
            selectFirst: css => select(markup, css, uri).first(),
            text: () => select(markup, ':root', uri).text(),
            html: () => markup,
            outerHtml: () => markup,
            toString: () => markup
        };
    }};
    globalThis.org = {jsoup: {Jsoup}};
    globalThis.Packages = {org: globalThis.org};
})();
