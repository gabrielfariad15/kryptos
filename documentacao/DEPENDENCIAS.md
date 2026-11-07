# Dependências do Kryptos

Toda dependência nova entra aqui antes de entrar no código (regra 8.6). AGPL e GPL ficam fora.

| Dependência | Onde | Licença | Versão | Situação | Motivo |
|---|---|---|---|---|---|
| vodozemac | nucleo | Apache-2.0 (conferido em 07/11/2026) | =0.11.2, sem os recursos padrão, só com `precomputed-tables` | em uso | Identidade do aparelho (conta Olm) e a conversa entre duas pessoas (sessão Olm versão 1, `nucleo/src/sessao.rs`); auditada pela Least Authority. A recusa de chave de ordem baixa (`NonContributoryKey`) existe desde a 0.10. A 0.11.2 (05/11/2026) corrige zeroização de chaves; o `libolm-compat` ficou fora porque não importamos conta do libolm |
| sha2 | nucleo | MIT OR Apache-2.0 (conferido em 07/11/2026) | 0.11 | em uso | SHA-256 do código de contato. É a mesma versão que o vodozemac já usa, então não entra crate novo na compilação |
| argon2 | nucleo | MIT OR Apache-2.0 (conferido em 07/11/2026) | =0.6.0, sem os recursos padrão, só `alloc` e `zeroize` | em uso | Argon2id do PIN no cofre da identidade. É exceção escrita à regra 8.1: nem o vodozemac nem o OpenMLS têm derivação de chave a partir de senha, e PIN de 6 dígitos sem custo cai em segundos. Não tem auditoria formal conhecida; conferido contra a implementação de referência em C (argon2-cffi) pelos vetores do `ferramentas/oraculos/cofre.py`. Traz o `blake2` (MIT OR Apache-2.0) |
| hkdf | nucleo | MIT OR Apache-2.0 (conferido em 07/11/2026) | 0.13 | em uso | Chave da conta a partir da chave do cofre e do nonce. Já vinha pelo vodozemac |
| zeroize | nucleo + frontend/app/rust | Apache-2.0 OR MIT (conferido em 07/11/2026) | 1.9 | em uso | Zerar PIN, chaves e conta em JSON depois do uso. Já vinha pelo vodozemac |
| getrandom | nucleo | MIT OR Apache-2.0 (conferido em 07/11/2026) | 0.4 | em uso | Sal e nonce do cofre, do gerador do sistema. Já vinha pelo vodozemac |
| serde, serde_json | nucleo | MIT OR Apache-2.0 (conferido em 07/11/2026) | 1.0 | em uso | Ler as chaves secretas da conta para a varredura do disco. Já vinham pelo vodozemac |
| base64 | nucleo | MIT OR Apache-2.0 (conferido em 07/11/2026) | 0.23 | em uso | Formas em base64 que a varredura procura. Já vinha pelo vodozemac |
| rxing | nucleo | Apache-2.0 (conferido em 07/11/2026) | =0.9.3, sem os recursos padrão, só `qrcode`, `decoders` e `encoding_rs` | em uso | Ler o QR do cartão no quadro da câmera. Escrito em Rust: o quadro é imagem vinda de fora, e leitor em C++ (zxing-cpp, `flutter_zxing`) seria a parte mais exposta do app. Na comparação de 07/11/2026, em 216 imagens degradadas iguais, leu 161, contra 163 do zxing-cpp e 91 do `rqrr` (recusado). Com os recursos padrão traria 264 crates. Otimizado também no build de debug (`profile.dev.package.rxing`): o quadro de 1280×720 cai de 31 ms para 3 ms no computador |
| qrcode | nucleo | MIT OR Apache-2.0 (conferido em 07/11/2026) | =0.14.1, sem os recursos padrão | em uso | Gerar a grade do QR do cartão, que a tela desenha. Sem os recursos padrão não traz crate nenhum |
| camera (pacote Dart) | frontend/app | BSD-3-Clause (conferido em 07/11/2026) | =0.12.2 | em uso | Quadros da câmera traseira para o leitor do núcleo e a prévia na tela. Só o plano Y em YUV 420, 480p, sem áudio. Sem serviço do Google. Exata: é o que entrega o quadro ao leitor |
| camera_android_camerax (indireta do camera) | frontend/app (Android) | BSD-3-Clause (conferido em 07/11/2026) | 0.7.6+1 | em uso | Implementação Android do `camera`, endossada desde o 0.11. Traz o CameraX 1.6.2 (`camera-core`, `camera2`, `lifecycle`, `video`; Apache-2.0, conferido no pom) e o Guava 33.5.0-android (Apache-2.0, conferido no pom). O pom do `camera-core` não depende de ML Kit nem de Play Services (o `camera-mlkit-vision` só aparece na lista de versões do grupo). O manifesto do plugin pede microfone e escrita no armazenamento, e o Android junta a leitura do armazenamento à escrita; o do app tira as três (`tools:node="remove"`), e o `instalar-no-celular.ps1` confere no app instalado |
| anyhow | frontend/app/rust | MIT OR Apache-2.0 (conferido em 07/11/2026) | 1.0 | em uso | Erro da ponte: chega ao Dart como `AnyhowException` com o código fixo do núcleo. Já vinha pelo flutter_rust_bridge |
| kryptos_protocolo (crate da workspace) | nucleo + backend/relay | do projeto | 0.1.0 | em uso | Formato da publicação e da retirada de chaves avulsas, igual no app e no relay. Depende só do vodozemac, que já estava na árvore: não entra crate de fora. Com ele, o relay passa a compilar o vodozemac (Apache-2.0), só para conferir a assinatura Ed25519 da publicação |
| kryptos_nucleo no relay (só testes) | backend/relay (dev) | do projeto | 0.1.0 | em uso | Teste de ponta a ponta: o dono publica, outro retira e a conversa abre. Não entra no binário do relay |
| openmls | nucleo | MIT | a definir | planejada (fase de grupos) | Grupos (MLS, RFC 9420) |
| flutter_rust_bridge (crate e pacote Dart) | frontend/app/rust + frontend/app | MIT (conferido em 06/11/2026) | =2.13.0 | em uso | Ponte entre o app Flutter e o núcleo em Rust. O gerador `flutter_rust_bridge_codegen` tem de estar na mesma versão |
| axum | backend/relay | MIT | 0.8.9 | em uso | Rotas HTTP do relay |
| tokio | backend/relay | MIT | 1.54.1 | em uso | Execução assíncrona e rede do relay |
| tracing | backend/relay | MIT | 0.1.44 | em uso | Registro de eventos do relay (nunca conteúdo de mensagem, regra 8.2) |
| tracing-subscriber | backend/relay | MIT | 0.3 | em uso | Saída do registro e filtro por `RUST_LOG` |
| tower | backend/relay (só testes) | MIT | 0.5 | em uso | Chamar as rotas nos testes sem abrir porta |
| http-body-util | backend/relay (só testes) | MIT | 0.1 | em uso | Ler o corpo da resposta nos testes |
| Python `argon2-cffi` e `cryptography` (só o oráculo) | ferramentas/oraculos (fora do app) | MIT; Apache-2.0 OR BSD-3-Clause | as do momento | só para recalcular vetores | Oráculo independente dos vetores do cofre, da sessão Olm (`olm.py`) e da publicação no relay (`publicacao.py`); o `ordem_baixa.py` usa só a biblioteca padrão. Não entram no app nem na suíte |
| IBM Plex Mono (fonte) | frontend/app/assets/fonts | OFL-1.1 | 2.5.0 | em uso | Fonte única do visual; embutida no app, com a licença em `OFL-IBM-Plex-Mono.txt` |
| camera_platform_interface, camera_avfoundation, camera_web, cross_file, flutter_plugin_android_lifecycle, stream_transform (indiretas do camera) | frontend/app | BSD-3-Clause (conferido em 07/11/2026, arquivo LICENSE de cada pacote) | 2.15.0; 0.11.1; 0.3.6; 0.3.5+5; 2.0.35; 2.1.2 | em uso | Interface comum, implementações de iPhone e web (não compiladas no Android) e apoio do plugin. Medido no `pubspec.lock`: o `camera` trouxe 9 pacotes, contando o `flutter_web_plugins` do SDK |
| integration_test, flutter_test, flutter_lints | frontend/app (só desenvolvimento) | BSD-3-Clause (Flutter) | do Flutter 3.47.9 | em uso | Testes da tela e teste da ponte no aparelho |

Conferência da árvore do núcleo em 07/11/2026, depois do cofre da identidade: 77 crates sem repetição (medido com `cargo tree -p kryptos_nucleo -e normal`; 79 contando os de compilação). Antes do cofre eram 75 e 77; entraram `argon2` e `blake2`. Todos MIT, Apache-2.0, BSD-3-Clause, Unicode-3.0 ou Unlicense. Nenhuma GPL ou AGPL.

Conferência da árvore do núcleo em 07/11/2026, depois do QR do cartão: 100 crates sem repetição, sem contar o próprio núcleo (medido com `cargo tree -p kryptos_nucleo -e normal` no Linux). Antes eram 77. Entraram 23: `rxing` e `qrcode`; `encoding_rs` e `codepage-437` (conjunto de caracteres do leitor); `regex`, `regex-automata`, `regex-syntax` e `aho-corasick`; `num` e cinco `num-*`; `chrono` e `iana-time-zone`; `once_cell`, `scopeguard`, `simdutf8`, `unicode-segmentation`, `core_detect` e `multiversion_no_op`. Todos MIT, Apache-2.0, BSD-3-Clause ou Unlicense. Nenhuma GPL ou AGPL. No Android o `iana-time-zone` deve trazer mais um crate (`android_system_properties`, MIT OR Apache-2.0; estimo, não medido).

Peso medido em 07/11/2026, na biblioteca da ponte em modo de distribuição, Linux x86_64, sem símbolos: 1.225.008 bytes sem o QR; 1.272.128 só com o gerador (mais 47 KB); 1.947.280 com gerador e leitor (mais 722 KB). No arm64 do celular estimo da mesma ordem, por processador incluído no APK.

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
| rqrr | MIT OR Apache-2.0, mas leu 91 de 216 imagens degradadas na comparação de 07/11/2026, contra 161 do `rxing` |
| mobile_scanner (pacote Dart) | No Android usa o ML Kit do Google, que pode mandar métricas de uso ao Google: fere a decisão de não falar com o Google |
| flutter_zxing (pacote Dart) | Reserva, não recusa: leu tão bem quanto o `rxing` (zxing-cpp, 163 de 216), mas é C++ lendo imagem vinda de fora e precisa compilar C++ com o NDK. Entra se o `rxing` não ler 9 em 10 no aparelho |
| vodozemac (pacote Dart do pub.dev) | AGPL-3.0, embora o crate em Rust seja Apache-2.0. Usar o crate com ponte própria |
| google_fonts (pacote Dart) | Baixa a fonte de servidor do Google na hora de rodar: fere a decisão de não falar com o Google e o funcionamento sem internet |
