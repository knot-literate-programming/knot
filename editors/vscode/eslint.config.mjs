import tseslint from 'typescript-eslint';

export default tseslint.config(
    tseslint.configs.recommended,
    {
        files: ['src/**/*.ts'],
        rules: {
            '@typescript-eslint/no-unused-vars': ['error', {
                argsIgnorePattern: '^_',
                varsIgnorePattern: '^_',
                caughtErrorsIgnorePattern: '^_',
            }],
        },
    },
    {
        ignores: ['out/**', 'node_modules/**'],
    }
);
