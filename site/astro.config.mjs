// @ts-check
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

export default defineConfig({
  site: "https://byteonicaai.github.io",
  base: "/hotdoc/",
  trailingSlash: "always",
  integrations: [
    starlight({
      title: "hotdoc",
      tagline: "Command syntax, in five seconds.",
      social: [{ icon: "github", label: "GitHub", href: "https://github.com/ByteonicaAI/hotdoc" }],
      sidebar: [
        { label: "Install", slug: "install" },
        {
          label: "Guide",
          items: [
            { label: "Getting started", slug: "guide/getting-started" },
            { label: "Daily use", slug: "guide/daily-use" },
            { label: "Packs", slug: "guide/packs" },
            { label: "Settings", slug: "guide/settings" },
          ],
        },
        {
          label: "Reference",
          items: [
            { label: "Keybindings", slug: "reference/keybindings" },
            { label: "CLI", slug: "reference/cli" },
            { label: "Troubleshooting", slug: "reference/troubleshooting" },
          ],
        },
      ],
    }),
  ],
});
