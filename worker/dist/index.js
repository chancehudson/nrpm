// index.mjs
var index_default = {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname.endsWith("/info/refs")) {
      return new Response("", {
        headers: { "Location": `https://api.nrpm.io${url.pathname}` },
        statusCode: 308
      });
    }
    return env.ASSETS.fetch(request);
  }
};
export {
  index_default as default
};
//# sourceMappingURL=index.js.map
