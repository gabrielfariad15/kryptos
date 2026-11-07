//! Funções do núcleo expostas ao app. Cada uma só repassa para `kryptos_nucleo`.

/// Versão do núcleo compilado dentro do app.
#[flutter_rust_bridge::frb(sync)]
pub fn versao_nucleo() -> String {
    kryptos_nucleo::versao().to_string()
}

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}
