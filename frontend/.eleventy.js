const md = require("markdown-it")({ html: true });

module.exports = (config) => {
  config.setBrowserSyncConfig({ files: ["dist/css/*.css"] });

  const proxy = (tokens, idx, options, _env, self) =>
    self.renderToken(tokens, idx, options);

  const defaultHeadingOpenRenderer = md.renderer.rules["heading_open"] || proxy;
  const defaultHeadingCloseRenderer =
    md.renderer.rules["heading_close"] || proxy;
  const increase = (tokens, idx) => {
    // Don't go smaller than 'h6'
    if (parseInt(tokens[idx].tag[1]) < 6) {
      tokens[idx].tag = tokens[idx].tag[0] + (parseInt(tokens[idx].tag[1]) + 1);
    }
  };
  md.renderer.rules["heading_open"] = function (
    tokens,
    idx,
    options,
    env,
    self,
  ) {
    increase(tokens, idx);
    return defaultHeadingOpenRenderer(tokens, idx, options, env, self);
  };
  md.renderer.rules["heading_close"] = function (
    tokens,
    idx,
    options,
    env,
    self,
  ) {
    increase(tokens, idx);
    return defaultHeadingCloseRenderer(tokens, idx, options, env, self);
  };

  config.setLibrary("md", md);

  return {
    markdownTemplateEngine: false,
    dataTemplateEngine: "njk",
    htmlTemplateEngine: "njk",
    dir: {
      input: "src",
      output: "dist",
    },
  };
};
