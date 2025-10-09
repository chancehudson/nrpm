export default {
  async fetch(request, env) {
    const url = new URL(request.url);

      if (url.pathname.endsWith("/info/refs")) {
          return new Response("", {
              headers: { "Location": `https://api.nrpm.io${url.pathname}${url.search}` },
              status: 308
          });
      }

    return env.ASSETS.fetch(request);
  },
};
