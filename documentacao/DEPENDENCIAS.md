# Dependências do Kryptos

Toda dependência nova entra aqui antes de entrar no código (regra 8.6). AGPL e GPL ficam fora.

| Dependência | Onde | Licença | Versão | Situação | Motivo |
|---|---|---|---|---|---|
| vodozemac | nucleo | Apache-2.0 | a definir | planejada | Conversa entre duas pessoas (Olm / Double Ratchet); auditada pela Least Authority |
| openmls | nucleo | MIT | a definir | planejada (fase de grupos) | Grupos (MLS, RFC 9420) |
| flutter_rust_bridge | frontend/app + nucleo | MIT / Apache-2.0 (conferir na adoção) | a definir | planejada | Ponte entre o app Flutter e o núcleo em Rust |

## Recusadas

| Dependência | Motivo |
|---|---|
| libsignal | AGPL-3.0: obrigaria abrir o código do app inteiro |
| vodozemac (pacote Dart do pub.dev) | AGPL-3.0, embora o crate em Rust seja Apache-2.0. Usar o crate com ponte própria |
