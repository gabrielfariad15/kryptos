# Dependências do Kryptos

Toda dependência nova entra aqui antes de entrar no código (regra 8.6). AGPL e GPL ficam fora.

| Dependência | Onde | Licença | Versão | Situação | Motivo |
|---|---|---|---|---|---|
| vodozemac | nucleo | Apache-2.0 | a definir | planejada | Conversa entre duas pessoas (Olm / Double Ratchet); auditada pela Least Authority |
| openmls | nucleo | MIT | a definir | planejada (fase de grupos) | Grupos (MLS, RFC 9420) |
| flutter_rust_bridge (crate e pacote Dart) | frontend/app/rust + frontend/app | MIT (conferido em 06/11/2026) | =2.13.0 | em uso | Ponte entre o app Flutter e o núcleo em Rust. O gerador `flutter_rust_bridge_codegen` tem de estar na mesma versão |
| axum | backend/relay | MIT | 0.8.9 | em uso | Rotas HTTP do relay |
| tokio | backend/relay | MIT | 1.54.1 | em uso | Execução assíncrona e rede do relay |
| tracing | backend/relay | MIT | 0.1.44 | em uso | Registro de eventos do relay (nunca conteúdo de mensagem, regra 8.2) |
| tracing-subscriber | backend/relay | MIT | 0.3 | em uso | Saída do registro e filtro por `RUST_LOG` |
| tower | backend/relay (só testes) | MIT | 0.5 | em uso | Chamar as rotas nos testes sem abrir porta |
| http-body-util | backend/relay (só testes) | MIT | 0.1 | em uso | Ler o corpo da resposta nos testes |
| IBM Plex Mono (fonte) | frontend/app/assets/fonts | OFL-1.1 | 2.5.0 | em uso | Fonte única do visual; embutida no app, com a licença em `OFL-IBM-Plex-Mono.txt` |
| integration_test, flutter_test, flutter_lints | frontend/app (só desenvolvimento) | BSD-3-Clause (Flutter) | do Flutter 3.47.9 | em uso | Testes da tela e teste da ponte no aparelho |

Conferência das dependências indiretas em 06/11/2026: 130 crates no `Cargo.lock` e 42 pacotes Dart, todos MIT, Apache-2.0, BSD, Zlib, Unlicense, BSL-1.0 ou Unicode-3.0. Nenhuma GPL ou AGPL. O `allo-isolate` (indireta do flutter_rust_bridge) não declara licença no `Cargo.toml`, mas traz o arquivo `LICENSE` com a Apache-2.0.

## Ferramentas da máquina (não vão para o app)

Instaladas por `ferramentas/instalar-ferramentas.ps1`, com versão fixa para o desktop e o notebook ficarem iguais:

| Ferramenta | Versão |
|---|---|
| Flutter (canal stable) | 3.47.9 (Dart 3.13.7) |
| NDK do Android | 29.0.14206865 (r29) |
| SDK do Android | platform 36, build-tools 36.0.0 |
| JDK | Temurin 17 |
| flutter_rust_bridge_codegen | 2.13.0 |

## Recusadas

| Dependência | Motivo |
|---|---|
| libsignal | AGPL-3.0: obrigaria abrir o código do app inteiro |
| vodozemac (pacote Dart do pub.dev) | AGPL-3.0, embora o crate em Rust seja Apache-2.0. Usar o crate com ponte própria |
| google_fonts (pacote Dart) | Baixa a fonte de servidor do Google na hora de rodar: fere a decisão de não falar com o Google e o funcionamento sem internet |
