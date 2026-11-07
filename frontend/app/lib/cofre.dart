// O que as telas sabem do cofre da identidade. Sem dependência da ponte: os testes
// de tela usam um cofre falso, e o de verdade (cofre_do_aparelho.dart) liga o núcleo
// em Rust ao Keystore do Android.

/// Identidade gravada cifrada no aparelho, aberta pelo PIN.
abstract class Cofre {
  /// Já existe identidade gravada neste aparelho?
  bool existe();

  /// Segundos que a próxima conferência de PIN vai esperar (muitas tentativas erradas).
  int esperaSegundos();

  /// Grava a identidade gerada nesta execução, protegida pelo [pin] (6 dígitos, 0 a 9).
  Future<void> criar(List<int> pin);

  /// Abre a identidade gravada e devolve o código de contato.
  /// Lança [PinIncorreto] se o PIN não abre o cofre.
  Future<String> abrir(List<int> pin);
}

/// O PIN não abriu o cofre (ou o arquivo foi alterado: a cifra não distingue).
class PinIncorreto implements Exception {
  const PinIncorreto();

  @override
  String toString() => 'PinIncorreto';
}

/// Outra falha do cofre, com o código fixo que veio do núcleo ou do Keystore
/// (por exemplo COFRE_CORROMPIDO, CHAVE_AUSENTE). Nunca traz PIN, chave nem código.
class FalhaDoCofre implements Exception {
  const FalhaDoCofre(this.codigo);

  final String codigo;

  @override
  String toString() => 'FalhaDoCofre($codigo)';
}
