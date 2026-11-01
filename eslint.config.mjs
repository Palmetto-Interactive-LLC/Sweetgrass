import tseslint from 'typescript-eslint';
import nextVitals from 'eslint-config-next/core-web-vitals';

export default [
  ...nextVitals,
  { ignores: ['out/**', '.next/**', 'server/target/**'] },
  { plugins: { '@typescript-eslint': tseslint.plugin }, rules: {
    // Existing effects need a separate behavioral refactor. Keep the new React
    // compiler diagnostic visible without changing behavior during dependency updates.
    "react-hooks/set-state-in-effect": "warn",
    "import/order": [
        "warn",
        {
            "groups": [
                "builtin",
                "external",
                "internal",
                [
                    "parent",
                    "sibling",
                    "index"
                ],
                "type"
            ],
            "pathGroups": [
                {
                    "pattern": "react",
                    "group": "builtin",
                    "position": "before"
                },
                {
                    "pattern": "next/**",
                    "group": "builtin",
                    "position": "after"
                },
                {
                    "pattern": "@/**",
                    "group": "internal",
                    "position": "before"
                }
            ],
            "pathGroupsExcludedImportTypes": [
                "react",
                "next"
            ],
            "newlines-between": "always",
            "alphabetize": {
                "order": "asc",
                "caseInsensitive": true
            }
        }
    ],
    "import/first": "error",
    "import/newline-after-import": "error",
    "import/no-duplicates": "error",
    "@typescript-eslint/no-unused-vars": [
        "warn",
        {
            "argsIgnorePattern": "^_",
            "varsIgnorePattern": "^_",
            "caughtErrorsIgnorePattern": "^_"
        }
    ]
} },
];
