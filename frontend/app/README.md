# Kryptos — app Flutter

App do Kryptos (Android primeiro). A tela fica aqui; as regras ficam no núcleo em Rust (`nucleo/`), chamado pela ponte em `rust/`.

## Pastas

- `lib/` — telas e tema. `lib/src/rust/` é gerado: não editar à mão.
- `rust/` — ponte fina (crate `kryptos_ponte`). Só repassa chamadas para o `kryptos_nucleo`.
- `rust_builder/` — gerado pelo `flutter_rust_bridge_codegen integrate` (cargokit). Compila o Rust dentro do build do Android.
- `assets/fonts/` — IBM Plex Mono embutida, com a licença OFL.
- `test/` — testes da tela no computador, sem o Rust.
- `integration_test/` — teste da ponte no celular, com o Rust de verdade.

## Mudou uma função em `rust/src/api/`?

Gere a ponte de novo, de dentro desta pasta:

```
flutter_rust_bridge_codegen generate
```

## Rodar

- Suíte completa: `ferramentas\suite.ps1` (com `-Aparelho` inclui o teste no celular).
- Instalar, abrir e capturar a tela: `ferramentas\instalar-no-celular.ps1`.
