import js from "@eslint/js";
import tseslint from "typescript-eslint";
import astro from "eslint-plugin-astro";
import reactHooks from "eslint-plugin-react-hooks";
import jsxA11y from "eslint-plugin-jsx-a11y";
import globals from "globals";

export default [
	{
		ignores: [
			"dist/",
			".astro/",
			".wrangler/",
			"node_modules/",
			"worker-configuration.d.ts",
		],
	},
	js.configs.recommended,
	...tseslint.configs.recommended,
	...astro.configs.recommended,
	{
		files: ["**/*.{js,mjs,cjs,jsx,ts,tsx}"],
		languageOptions: {
			globals: { ...globals.browser, ...globals.node },
		},
		plugins: {
			"react-hooks": reactHooks,
			"jsx-a11y": jsxA11y,
		},
		rules: {
			...reactHooks.configs.recommended.rules,
			"@typescript-eslint/no-unused-vars": [
				"warn",
				{ argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
			],
		},
	},
];
