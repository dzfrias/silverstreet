module.exports = (config) => {
  config.setBrowserSyncConfig({ files: ["dist/css/*.css"] });

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
