# Dependências do Kryptos

Toda dependência nova entra aqui antes de entrar no código (regra 8.6). AGPL e GPL ficam fora.

| Dependência | Onde | Licença | Versão | Situação | Motivo |
|---|---|---|---|---|---|
| vodozemac | nucleo | Apache-2.0 (conferido em 07/11/2026) | =0.11.2, sem os recursos padrão, só com `precomputed-tables` | em uso | Identidade do aparelho (conta Olm) e, adiante, a conversa entre duas pessoas (Double Ratchet); auditada pela Least Authority. A 0.11.2 (05/11/2026) corrige zeroização de chaves; o `libolm-compat` ficou fora porque não importamos conta do libolm |
| sha2 | nucleo | MIT OR Apache-2.0 (conferido em 07/11/2026) | 0.11 | em uso | SHA-256 do código de contato. É a mesma versão que o vodozemac já usa, então não entra crate novo na compilação |
| argon2 | nucleo | MIT OR Apache-2.0 (conferido em 07/11/2026) | =0.6.0, sem os recursos padrão, só `alloc` e `zeroize` | em uso | Argon2id do PIN no cofre da identidade. É exceção escrita à regra 8.1: nem o vodozemac nem o OpenMLS têm derivação de chave a partir de senha, e PIN de 6 dígitos sem custo cai em segundos. Não tem auditoria formal conhecida; conferido contra a implementação de referência em C (argon2-cffi) pelos vetores do `ferramentas/oraculos/cofre.py`. Traz o `blake2` (MIT OR Apache-2.0) |
| hkdf | nucleo | MIT OR Apache-2.0 (conferido em 07/11/2026) | 0.13 | em uso | Chave da conta a partir da chave do cofre e do nonce. Já vinha pelo vodozemac |
| zeroize | nucleo + frontend/app/rust | Apache-2.0 OR MIT (conferido em 07/11/2026) | 1.9 | em uso | Zerar PIN, chaves e conta em JSON depois do uso. Já vinha pelo vodozemac |
| getrandom | nucleo | MIT OR Apache-2.0 (conferido em 07/11/2026) | 0.4 | em uso | Sal e nonce do cofre, do gerador do sistema. Já vinha pelo vodozemac |
| serde, serde_json | nucleo | MIT OR Apache-2.0 (conferido em 07/11/2026) | 1.0 | em uso | Ler as chaves secretas da conta para a varredura do disco. Já vinham pelo vodozemac |
| base64 | nucleo | MIT OR Apache-2.0 (conferido em 07/11/2026) | 0.23 | em uso | Formas em base64 que a varredura procura. Já vinha pelo vodozemac |
| anyhow | frontend/app/rust | MIT OR Apache-2.0 (conferido em 07/11/2026) | 1.0 | em uso | Erro da ponte: chega ao Dart como `AnyhowException` com o código fixo do núcleo. Já vinha pelo flutter_rust_bridge |
| openmls | nucleo | MIT | a definir | planejada (fase de grupos) | Grupos (MLS, RFC 9420) |
| flutter_rust_bridge (crate e pacote Dart) | frontend/app/rust + frontend/app | MIT (conferido em 06/11/2026) | =2.13.0 | em uso | Ponte entre o app Flutter e o núcleo em Rust. O gerador `flutter_rust_bridge_codegen` tem de estar na mesma versão |
| axum | backend/relay | MIT | 0.8.9 | em uso | Rotas HTTP do relay |
| tokio | backend/relay | MIT | 1.54.1 | em uso | Execução assíncrona e rede do relay |
| tracing | backend/relay | MIT | 0.1.44 | em uso | Registro de eventos do relay (nunca conteúdo de mensagem, regra 8.2) |
| tracing-subscriber | backend/relay | MIT | 0.3 | em uso | Saída do registro e filtro por `RUST_LOG` |
| tower | backend/relay (só testes) | MIT | 0.5 | em uso | Chamar as rotas nos testes sem abrir porta |
| http-body-util | backend/relay (só testes) | MIT | 0.1 | em uso | Ler o corpo da resposta nos testes |
| Python `argon2-cffi` e `cryptography` (só o oráculo) | ferramentas/oraculos (fora do app) | MIT; Apache-2.0 OR BSD-3-Clause | as do momento | só para recalcular vetores | Oráculo independente dos vetores do cofre. Não entram no app nem na suíte |
| IBM Plex Mono (fonte) | frontend/app/assets/fonts | OFL-1.1 | 2.5.0 | em uso | Fonte única do visual; embutida no app, com a licença em `OFL-IBM-Plex-Mono.txt` |
| integration_test, flutter_test, flutter_lints | frontend/app (só desenvolvimento) | BSD-3-Clause (Flutter) | do Flutter 3.47.9 | em uso | Testes da tela e teste da ponte no aparelho |

Conferência da árvore do núcleo em 07/11/2026, depois do cofre da identidade: 77 crates sem repetição (medido com `cargo tree -p kryptos_nucleo -e normal`; 79 contando os de compilação). Antes do cofre eram 75 e 77; entraram `argon2` e `blake2`. Todos MIT, Apache-2.0, BSD-3-Clause, Unicode-3.0 ou Unlicense. Nenhuma GPL ou AGPL.

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
