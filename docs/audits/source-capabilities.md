# Book source static capability inventory

Run: node scripts/audit-source-capabilities.mjs

Offline lexical signals in executable rule/URL/header/jsLib/login fields; exact canonical JSON deduplication. Comments, strings and overridden aliases can cause false positives; computed APIs, remote libraries and indirect calls can be missed. Binding presence does not establish compatibility. java.get/put can mean variables, not HTTP. URL counts union versions; these are not runnable-source coverage.

{"records":961,"distinct_versions":961,"distinct_nonempty_urls":926,"urls_with_multiple_versions":34,"missing_url_versions":0}

| Dataset | Records | Distinct versions | SHA-256 |
|---|---:|---:|---|
| booksources.json | 22 | 22 | 24fe11c400ddc8754703ff0c4d8a249baf024da3a15563de805cc134140959a0 |
| booksourcesmost.json | 939 | 939 | 500db35e8d2b11433ae01860c37fcf10f5d06ab0854aba3a2bb666bd8d46ef01 |

## Feature signals

| Signal | Distinct versions | Distinct URLs |
|---|---:|---:|
| js_marker | 952 | 919 |
| cookie_jar_enabled | 501 | 483 |
| field_header | 361 | 345 |
| java_api | 258 | 248 |
| field_loginUrl | 183 | 178 |
| js_http_or_get_put | 174 | 166 |
| js_http_explicit | 145 | 139 |
| login_url_http | 136 | 134 |
| source_api | 129 | 125 |
| webview_signal | 95 | 92 |
| rule_variable_api | 71 | 69 |
| cookie_api | 70 | 68 |
| response_method | 36 | 33 |
| login_url_other | 31 | 30 |
| field_jsLib | 20 | 17 |
| field_loginUi | 18 | 16 |
| login_url_js | 16 | 14 |
| java_interop | 15 | 15 |
| field_concurrentRate | 14 | 13 |
| cache_api | 8 | 8 |
| field_loginCheckJs | 7 | 7 |

## API signals

Binding presence does not establish argument, result or state compatibility.

| API | Distinct versions | Distinct URLs | Binding present |
|---|---:|---:|---|
| java.ajax | 135 | 131 | yes (review semantics) |
| java.getString | 91 | 87 | no direct registration |
| source.getKey | 66 | 65 | yes (review semantics) |
| cookie.removeCookie | 64 | 63 | yes (review semantics) |
| java.put | 63 | 61 | yes (review semantics) |
| java.get | 58 | 58 | yes (review semantics) |
| java.log | 57 | 54 | no direct registration |
| java.timeFormat | 39 | 38 | yes (review semantics) |
| java.toast | 37 | 35 | no direct registration |
| book.name | 27 | 26 | no direct registration |
| java.longToast | 23 | 22 | no direct registration |
| source.getVariable | 22 | 21 | no direct registration |
| source.key | 22 | 22 | yes (review semantics) |
| java.base64Decode | 21 | 21 | yes (review semantics) |
| java.startBrowserAwait | 21 | 21 | no direct registration |
| java.md5Encode | 19 | 18 | yes (review semantics) |
| java.post | 18 | 16 | yes (review semantics) |
| java.startBrowser | 15 | 14 | no direct registration |
| java.toNumChapter | 15 | 15 | yes (review semantics) |
| source.bookSourceComment | 15 | 15 | no direct registration |
| source.setVariable | 15 | 14 | no direct registration |
| book.midukanshu | 14 | 14 | no direct registration |
| book.origin | 14 | 13 | no direct registration |
| book.bookUrl | 13 | 13 | no direct registration |
| chapter.title | 13 | 13 | no direct registration |
| book.durChapterIndex | 12 | 12 | no direct registration |
| source.getLoginInfoMap | 12 | 10 | no direct registration |
| cookie.getCookie | 11 | 11 | yes (review semantics) |
| java.base64Encode | 11 | 10 | yes (review semantics) |
| java.setContent | 11 | 10 | no direct registration |
| source.bookSourceUrl | 11 | 11 | no direct registration |
| book.getVariable | 10 | 10 | no direct registration |
| chapter.index | 10 | 10 | no direct registration |
| book.author | 9 | 9 | no direct registration |
| java.aesBase64DecodeToString | 9 | 8 | yes (review semantics) |
| java.t2s | 9 | 9 | no direct registration |
| source.putLoginHeader | 9 | 7 | no direct registration |
| book.durChapterTitle | 8 | 8 | no direct registration |
| java.webView | 8 | 7 | no direct registration |
| java.createSymmetricCrypto | 7 | 6 | no direct registration |
| java.getElement | 7 | 7 | no direct registration |
| java.getStringList | 7 | 7 | no direct registration |
| java.lang | 7 | 7 | no direct registration |
| book.tiexue | 6 | 6 | no direct registration |
| cache.get | 6 | 6 | yes (review semantics) |
| java.encodeURI | 6 | 6 | yes (review semantics) |
| java.getElements | 6 | 6 | no direct registration |
| java.util | 6 | 6 | no direct registration |
| book.intro | 5 | 5 | no direct registration |
| cache.put | 5 | 5 | yes (review semantics) |
| cookie.setCookie | 5 | 5 | no direct registration |
| java.ajaxAll | 5 | 5 | no direct registration |
| java.base64DecodeToByteArray | 5 | 4 | no direct registration |
| java.getCookie | 5 | 5 | no direct registration |
| java.getWebViewUA | 5 | 5 | no direct registration |
| java.refreshTocUrl | 5 | 4 | no direct registration |
| book.kind | 4 | 4 | no direct registration |
| book.type | 4 | 4 | no direct registration |
| book.zri | 4 | 4 | no direct registration |
| java.androidId | 4 | 4 | yes (review semantics) |
| java.connect | 4 | 3 | no direct registration |
| java.hexDecodeToString | 4 | 4 | yes (review semantics) |
| java.refreshExplore | 4 | 3 | no direct registration |
| book.do | 3 | 3 | no direct registration |
| book.id | 3 | 3 | no direct registration |
| book.title | 3 | 3 | no direct registration |
| book.tocUrl | 3 | 3 | no direct registration |
| cache.delete | 3 | 3 | no direct registration |
| chapter.chapter_name | 3 | 3 | no direct registration |
| chapter.do | 3 | 3 | no direct registration |
| chapter.htmlContent | 3 | 3 | no direct registration |
| chapter.nid | 3 | 3 | no direct registration |
| chapter.sort | 3 | 3 | no direct registration |
| chapter.wordCount | 3 | 3 | no direct registration |
| java.getVerificationCode | 3 | 3 | no direct registration |
| source.header | 3 | 3 | no direct registration |
| source.loginUrl | 3 | 3 | no direct registration |
| book.coverUrl | 2 | 2 | no direct registration |
| book.finished | 2 | 2 | no direct registration |
| book.lastChapterName | 2 | 2 | no direct registration |
| book.map | 2 | 2 | no direct registration |
| book.putCustomVariable | 2 | 2 | no direct registration |
| book.totalwords | 2 | 2 | no direct registration |
| cache.deleteMemory | 2 | 2 | no direct registration |
| cache.getFromMemory | 2 | 2 | no direct registration |
| cache.putMemory | 2 | 2 | no direct registration |
| chapter.body | 2 | 2 | no direct registration |
| chapter.js | 2 | 1 | no direct registration |
| cookie.getKey | 2 | 1 | no direct registration |
| cookie.setWebCookie | 2 | 2 | no direct registration |
| java.ajaxTestAll | 2 | 2 | no direct registration |
| java.desEncodeToBase64String | 2 | 2 | no direct registration |
| java.getStrResponse | 2 | 2 | no direct registration |
| java.getUserAgent | 2 | 2 | no direct registration |
| java.HMacHex | 2 | 2 | yes (review semantics) |
| java.openVideoPlayer | 2 | 2 | no direct registration |
| java.randomUUID | 2 | 2 | no direct registration |
| java.refreshBookUrl | 2 | 2 | no direct registration |
| java.ruleUrl | 2 | 2 | no direct registration |
| java.s2t | 2 | 2 | no direct registration |
| java.startBrowserAwaitAwait | 2 | 2 | no direct registration |
| java.timeFormatUTC | 2 | 2 | yes (review semantics) |
| java.webview | 2 | 2 | no direct registration |
| source.bookSourceName | 2 | 2 | no direct registration |
| source.getLoginHeader | 2 | 2 | no direct registration |
| source.getLoginHeaderMap | 2 | 1 | no direct registration |
| source.lastUpdateTime | 2 | 2 | no direct registration |
| source.refreshExplore | 2 | 2 | no direct registration |
| source.refreshJSLib | 2 | 2 | no direct registration |
| book.canUpdate | 1 | 1 | no direct registration |
| book.classId | 1 | 1 | no direct registration |
| book.content_updated_at | 1 | 1 | no direct registration |
| book.contributors | 1 | 1 | no direct registration |
| book.description | 1 | 1 | no direct registration |
| book.docs | 1 | 1 | no direct registration |
| book.douban | 1 | 1 | no direct registration |
| book.midureader | 1 | 1 | no direct registration |
| book.qq | 1 | 1 | no direct registration |
| book.sbkk8 | 1 | 1 | no direct registration |
| book.toc | 1 | 1 | no direct registration |
| book.totalChapterNum | 1 | 1 | no direct registration |
| book.vik | 1 | 1 | no direct registration |
| cache.getFile | 1 | 1 | no direct registration |
| cache.putFile | 1 | 1 | no direct registration |
| chapter.imageUrls | 1 | 1 | no direct registration |
| chapter.substring | 1 | 1 | no direct registration |
| chapter.url | 1 | 1 | no direct registration |
| java.bytesToStr | 1 | 1 | no direct registration |
| java.deviceID | 1 | 1 | yes (review semantics) |
| java.digestHex | 1 | 1 | yes (review semantics) |
| java.head | 1 | 1 | no direct registration |
| java.hexDecodeToByteArray | 1 | 1 | no direct registration |
| java.io | 1 | 1 | no direct registration |
| java.md5Encode16 | 1 | 1 | yes (review semantics) |
| java.refreshContent | 1 | 1 | no direct registration |
| source.bookSourceType | 1 | 1 | no direct registration |
| source.concat | 1 | 1 | no direct registration |
| source.putLoginInfo | 1 | 1 | no direct registration |
| source.split | 1 | 1 | no direct registration |
| source.variableComment | 1 | 1 | no direct registration |
