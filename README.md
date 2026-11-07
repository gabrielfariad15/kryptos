# Kryptos

Mensageiro privado da Torion Systems: criptografia de ponta a ponta, mensagens que queimam depois de lidas e contato por código, sem número de celular.

## Pastas

- `documentacao/` — decisões, arquitetura e telas aprovadas
- `ferramentas/` — scripts do projeto, rodados por uma linha; `ambiente.json` guarda os fatos fixos da máquina
- `backend/` — o que roda no servidor
  - `relay/` — servidor de repasse (Rust): guarda a mensagem cifrada até a entrega e apaga
  - `atestacao/` — verificação da atestação por hardware dos aparelhos (Rust)
  - `infra/` — implantação do servidor
- `frontend/` — o que o usuário vê
  - `app/` — aplicativo Flutter (Android primeiro)
  - `design/` — paleta, fontes e referências das telas
- `nucleo/` — motor do aplicativo em Rust: criptografia, protocolo e queima. Roda no aparelho, não no servidor. O `frontend/app` chama o núcleo pelo flutter_rust_bridge.

## Fase atual

Fase 1: Android, conversa entre duas pessoas. Ver `documentacao/DECISOES-FASE-0.md`.