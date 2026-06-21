import js from "@eslint/js";
import tseslint from "typescript-eslint";
import sveltePlugin from "eslint-plugin-svelte";
import svelteParser from "svelte-eslint-parser";
import prettier from "eslint-config-prettier";
import globals from "globals";

const [tsBase, tsEslintRecommended, tsRecommendedTypeChecked] =
  tseslint.configs.recommendedTypeChecked;

export default [
  // Global ignores
  {
    ignores: [
      "dist/**",
      "node_modules/**",
      "target/**",
      "src-tauri/target/**",
      "src-tauri/gen/**",
      "src-tauri/Cargo.lock",
      "coverage/**",
      ".svelte-kit/**",
      "build/**",
      "release/**",
      "pnpm-lock.yaml",
    ],
  },

  // Base JS recommended for everything
  js.configs.recommended,

  // Register the @typescript-eslint plugin globally
  tsBase,

  // Application source: type-checked TS rules, with parser services
  {
    files: ["src/**/*.{ts,js,svelte}"],
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
      globals: { ...globals.browser },
    },
    rules: {
      ...tsEslintRecommended.rules,
      ...tsRecommendedTypeChecked.rules,
      "@typescript-eslint/consistent-type-imports": [
        "error",
        { prefer: "type-imports", fixStyle: "inline-type-imports" },
      ],
      "@typescript-eslint/no-unused-vars": [
        "warn",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
      "no-console": ["warn", { allow: ["warn", "error", "info"] }],
    },
  },

  // Loose config for project config files (eslint, vite, svelte configs)
  // These are not in the app tsconfig — no project service.
  {
    files: ["*.config.{js,ts,mjs,cjs}", "*.cjs", "*.mjs"],
    languageOptions: {
      globals: { ...globals.node },
    },
    rules: {
      "@typescript-eslint/no-unsafe-assignment": "off",
      "@typescript-eslint/no-unsafe-member-access": "off",
      "@typescript-eslint/no-unsafe-call": "off",
      "@typescript-eslint/no-unsafe-return": "off",
      "@typescript-eslint/require-await": "off",
      "no-console": "off",
    },
  },

  // Svelte: parser + plugin for .svelte files
  {
    files: ["**/*.svelte"],
    languageOptions: {
      parser: svelteParser,
      parserOptions: {
        parser: tseslint.parser,
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
        extraFileExtensions: [".svelte"],
      },
    },
    plugins: {
      svelte: sveltePlugin,
    },
    rules: {
      ...sveltePlugin.configs.recommended.rules,
      ...sveltePlugin.configs["flat/recommended"].rules,
    },
  },

  // Prettier must come last to disable conflicting stylistic rules
  prettier,
];
