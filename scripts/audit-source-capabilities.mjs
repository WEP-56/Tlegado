import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';

// Offline inventory: do not execute book-source scripts or fetch their URLs.
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const files = process.argv.slice(2).length ? process.argv.slice(2) : ['booksources.json', 'booksourcesmost.json'];
const hash = value => crypto.createHash('sha256').update(value).digest('hex');
const canonical = v => Array.isArray(v) ? v.map(canonical) : v && typeof v === 'object'
  ? Object.fromEntries(Object.keys(v).sort().map(k => [k, canonical(v[k])])) : v;
const activeField = k => /^(rule|bookSource(Search|Book|Content|Chapter|Toc)|searchUrl$|exploreUrl$|header$|jsLib$|loginUrl$|loginUi$|loginCheckJs$)/i.test(k);
function leaves(value, prefix) {
  if (typeof value === 'string') return [[prefix, value]];
  if (!value || typeof value !== 'object') return [];
  return Object.entries(value).flatMap(([k,v]) => leaves(v, prefix + '.' + k));
}
const patterns = {
  js_marker: /@js:|<js>|\{\{/i,
  java_api: /\bjava\s*(?:\.|\[)/,
  js_http_or_get_put: /\bjava\s*\.\s*(?:ajax|ajaxAll|connect|get|post|head|put|delete)\s*\(/,
  js_http_explicit: /\bjava\s*\.\s*(?:ajax|ajaxAll|connect|post|head)\s*\(/,
  source_api: /\bsource\s*(?:\.|\[)/,
  cookie_api: /\bcookie\s*(?:\.|\[)/,
  cache_api: /\bcache\s*(?:\.|\[)|\bjava\s*\.\s*(?:getCache|putCache)\s*\(/,
  rule_variable_api: /\bjava\s*\.\s*(?:get|put)\s*\(/,
  webview_signal: /webView|webJs|webViewDelayTime|\bjava\s*\.\s*(?:getWebView[A-Za-z]*|startBrowser[A-Za-z]*)\s*\(/,
  java_interop: /\b(?:Packages\.|importClass\s*\(|importPackage\s*\(|Java\s*\.\s*type\s*\(|java\s*\.\s*(?:lang|util|net|security|io)\s*\.|javax\.)/,
  response_method: /\.\s*(?:body|code|headers|header|url|isSuccessful)\s*\(/,
};
function inspect(source) {
  const fields = Object.entries(source).filter(([k]) => activeField(k)).flatMap(([k,v]) => leaves(v,k));
  const features = {};
  for (const [name,re] of Object.entries(patterns)) {
    const hits = fields.filter(([,s]) => re.test(s)).map(([k]) => k);
    if (hits.length) features[name] = hits;
  }
  for (const name of ['loginUrl','loginUi','loginCheckJs','jsLib','concurrentRate','header']) {
    if (source[name] != null && String(source[name]).trim()) features['field_' + name] = [name];
  }
  if (source.enabledCookieJar === true) features.cookie_jar_enabled = ['enabledCookieJar'];
  if (typeof source.loginUrl === 'string' && source.loginUrl.trim()) {
    const login = source.loginUrl.trim();
    const kind = /^https?:\/\//i.test(login) ? 'http' : /^(?:@js:|<js>)|\bfunction\s+login\s*\(/i.test(login) ? 'js' : 'other';
    features['login_url_' + kind] = ['loginUrl'];
  }
  const apis = {};
  for (const [field,s] of fields) {
    const re = /\b(java|source|cookie|cache|book|chapter)\s*(?:\.\s*([A-Za-z_$][\w$]*)|\[\s*["']([A-Za-z_$][\w$]*)["']\s*\])/g;
    for (const m of s.matchAll(re)) (apis[m[1]+'.'+(m[2]||m[3])] ??= new Set()).add(field);
  }
  return {features,apis:Object.fromEntries(Object.entries(apis).map(([k,v]) => [k,[...v].sort()]))};
}
const datasets = [], versions = new Map();
for (const file of files) {
  const bytes = fs.readFileSync(path.resolve(root,file));
  const parsed = JSON.parse(bytes.toString('utf8').replace(/^\uFEFF/,''));
  const sources = Array.isArray(parsed) ? parsed : [parsed];
  if (sources.some(s => !s || typeof s !== 'object' || Array.isArray(s))) throw Error('Invalid source record: '+file);
  const ids = [];
  for (const [index,source] of sources.entries()) {
    const id = hash(JSON.stringify(canonical(source))); ids.push(id);
    if (!versions.has(id)) versions.set(id,{id,name:source.bookSourceName||'',url:source.bookSourceUrl||'',locations:[],...inspect(source)});
    versions.get(id).locations.push({file,index});
  }
  datasets.push({file,sha256:hash(bytes),records:sources.length,distinct_versions:new Set(ids).size,ids});
}
const rows = [...versions.values()];
const rust = fs.readFileSync(path.join(root,'crates/legado-core/src/parser/js.rs'),'utf8');
const bindings = new Set([...rust.matchAll(/(java|source|cookie|cache)_obj\s*\.\s*set\s*\(\s*"([^"]+)"/g)].map(m => m[1]+'.'+m[2]));
function aggregate(items,property) {
  const map = new Map();
  for (const row of items) for (const key of Object.keys(row[property])) {
    if (!map.has(key)) map.set(key,[]); map.get(key).push(row);
  }
  return [...map].map(([name,sources]) => ({name,versions:sources.length,urls:new Set(sources.map(s=>s.url).filter(Boolean)).size,
    ...(property==='apis'?{binding_present:bindings.has(name)}:{}),
    examples:sources.slice(0,4).map(s=>({name:s.name,source_id:s.id,fields:s[property][name]}))
  })).sort((a,b)=>b.versions-a.versions||a.name.localeCompare(b.name));
}
const byUrl = new Map();
for (const row of rows) if (row.url) { if (!byUrl.has(row.url)) byUrl.set(row.url,[]); byUrl.get(row.url).push(row.id); }
const report = {
  schema_version:1,
  method:'Offline lexical signals in executable rule/URL/header/jsLib/login fields; exact canonical JSON deduplication. Comments, strings and overridden aliases can cause false positives; computed APIs, remote libraries and indirect calls can be missed. Binding presence does not establish compatibility. java.get/put can mean variables, not HTTP. URL counts union versions; these are not runnable-source coverage.',
  totals:{records:datasets.reduce((n,d)=>n+d.records,0),distinct_versions:rows.length,distinct_nonempty_urls:byUrl.size,urls_with_multiple_versions:[...byUrl.values()].filter(v=>v.length>1).length,missing_url_versions:rows.filter(r=>!r.url).length},
  datasets:datasets.map(({ids,...d})=>({...d,features:aggregate([...new Set(ids)].map(id=>versions.get(id)),'features')})),
  features:aggregate(rows,'features'),apis:aggregate(rows,'apis'),registered_bindings:[...bindings].sort(),sources:rows,
};
const out=path.join(root,'docs/audits'); fs.mkdirSync(out,{recursive:true});
fs.writeFileSync(path.join(out,'source-capabilities.json'),JSON.stringify(report,null,2)+'\n');
const md=['# Book source static capability inventory','','Run: node scripts/audit-source-capabilities.mjs','',report.method,'',
  JSON.stringify(report.totals),'','| Dataset | Records | Distinct versions | SHA-256 |','|---|---:|---:|---|',
  ...datasets.map(d=>'| '+[d.file,d.records,d.distinct_versions,d.sha256].join(' | ')+' |'),'',
  '## Feature signals','','| Signal | Distinct versions | Distinct URLs |','|---|---:|---:|',
  ...report.features.map(f=>'| '+[f.name,f.versions,f.urls].join(' | ')+' |'),'',
  '## API signals','','Binding presence does not establish argument, result or state compatibility.','',
  '| API | Distinct versions | Distinct URLs | Binding present |','|---|---:|---:|---|',
  ...report.apis.map(a=>'| '+[a.name,a.versions,a.urls,a.binding_present?'yes (review semantics)':'no direct registration'].join(' | ')+' |'),''];
fs.writeFileSync(path.join(out,'source-capabilities.md'),md.join('\n'));
console.log(JSON.stringify({totals:report.totals,datasets:datasets.map(({ids,...d})=>d),features:report.features.map(({examples,...f})=>f),top_apis:report.apis.slice(0,45).map(({examples,...a})=>a)},null,2));
