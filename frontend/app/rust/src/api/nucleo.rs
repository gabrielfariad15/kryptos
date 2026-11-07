//! Funções do núcleo expostas ao app. Cada uma só repassa para `kryptos_nucleo`.

/// Versão do núcleo compilado dentro do app.
#[flutter_rust_bridge::frb(sync)]
pub fn versao_nucleo() -> String {
    kryptos_nucleo::versao().to_string()
}

/// Gera a identidade do aparelho (uma vez por execução) e devolve o código de
/// contato no formato XXXX-XXXX. A chave fica no núcleo; só o código atravessa.
#[flutter_rust_bridge::frb(sync)]
pub fn gerar_codigo_contato() -> String {
    kryptos_nucleo::aparelho::gerar_identidade().como_texto()
}

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}
