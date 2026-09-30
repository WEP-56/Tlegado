// Host callbacks exchange JSON; this layer preserves Legado's distinct API contracts.
(function () {
    java.ensureGlobalVariable = function(k,v) {
        const error=java.__ensureGlobalVariable(String(k),String(v));
        if(error) throw Error(error);
        return true;
    };
    function response(raw) {
        function callable(value) {
            const f = function () { return value; };
            f.toJSON = function () { return value; };
            f.valueOf = function () { return value; };
            f.toString = function () { return String(value); };
            f[Symbol.toPrimitive] = function () { return value; };
            if (typeof value === 'string') {
                for (const name of Object.getOwnPropertyNames(String.prototype)) {
                    if (name !== 'constructor' && typeof value[name] === 'function' && !(name in f)) {
                        f[name] = value[name].bind(value);
                    }
                }
            }
            return f;
        }
        const list = raw.headers || [];
        function header(name) {
            const pair = list.find(x => x[0].toLowerCase() === String(name).toLowerCase());
            return pair ? pair[1] : null;
        }
        const map = {};
        for (const [key, value] of list) map[key] = value;
        Object.defineProperty(map, 'get', {value: header});
        Object.defineProperty(map, 'values', {value: name => list.filter(x => x[0].toLowerCase() === String(name).toLowerCase()).map(x => x[1])});
        const headers = () => map;
        headers.get = header;
        headers.toJSON = () => list;
        return {
            body: callable(raw.body), code: callable(raw.code), url: callable(raw.url),
            isSuccessful: callable(raw.isSuccessful), headers, header,
            statusCode: () => raw.code,
            toJSON: () => raw,
            toString: () => raw.body
        };
    }
    function request(url, method, body, headers) {
        const reply = JSON.parse(__sourceRequest(String(url), method, body == null ? null : String(body),
            typeof headers === 'string' ? headers : JSON.stringify(headers || {})));
        if (!reply.ok) throw new Error(reply.error);
        return reply.response;
    }
    java.ajax = url => request(url, '', null, {}).body;
    java.connect = (url, headers) => response(request(url, '', null, headers));
    java.get = function (key, headers) {
        if (arguments.length === 1) return __ruleGet(String(key));
        return response(request(key, 'GET', null, headers));
    };
    java.put = (key, value) => { __rulePut(String(key), String(value)); return String(value); };
    java.post = (url, body, headers) => response(request(url, 'POST', body, headers));
    java.head = (url, headers) => response(request(url, 'HEAD', null, headers));
    // Retain JSON serialization and common legacy result.body.replace(...) usage.
    if (result && typeof result === 'object' && typeof result.body === 'string' && typeof result.code === 'number') {
        result = response(result);
    }
})();
