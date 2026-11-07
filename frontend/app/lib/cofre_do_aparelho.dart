// Cofre de verdade: núcleo em Rust (Argon2id e cifra) + Keystore do Android (HMAC).
// Formato e etapas: documentacao/arquitetura/cofre-da-identidade.md.

import 'package:flutter/services.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';

import 'cofre.dart';
import 'src/rust/api/nucleo.dart' as nucleo;

/// Alias da chave do Keystore que protege a identidade do app.
const aliasDaIdentidade = 'kryptos-identidade';

/// Pastas do app informadas pelo Android.
class PastasDoApp {
  const PastasDoApp({required this.semBackup, required this.dados});

  /// `noBackupFilesDir`: fora de qualquer backup. É onde o cofre fica.
  final String semBackup;

  /// `dataDir`: a pasta inteira do app, para a varredura do teste no aparelho.
  final String dados;
}

/// Canal para a chave HMAC do Keystore (MainActivity.kt / ChaveDoAparelho.kt).
class ChaveDoAparelho {
  ChaveDoAparelho([MethodChannel? canal])
    : _canal = canal ?? const MethodChannel('kryptos/chave-do-aparelho');

  final MethodChannel _canal;

  Future<PastasDoApp> pastas() async {
    final m = await _canal.invokeMapMethod<String, String>('pastas');
    final semBackup = m?['semBackup'];
    final dados = m?['dados'];
    if (semBackup == null || dados == null) {
      throw const FalhaDoCofre('PASTAS_AUSENTES');
    }
    return PastasDoApp(semBackup: semBackup, dados: dados);
  }

  /// Troca a chave do [alias] por uma nova e devolve onde ela ficou
  /// (STRONGBOX, TEE, SOFTWARE).
  Future<String> criar(String alias) async =>
      await _canal.invokeMethod<String>('criar', {'alias': alias}) ?? '';

  Future<Uint8List> hmac(String alias, Uint8List dados) async {
    final r = await _canal.invokeMethod<Uint8List>('hmac', {
      'alias': alias,
      'dados': dados,
    });
    if (r == null || r.length != 32) throw const FalhaDoCofre('HMAC_INVALIDO');
    // No aparelho o canal entrega os bytes somente leitura (buffer do motor do
    // Flutter). A cópia é nossa e pode ser zerada depois do uso; o original fica
    // até o coletor de lixo passar.
    return Uint8List.fromList(r);
  }

  Future<String> nivel(String alias) async =>
      await _canal.invokeMethod<String>('nivel', {'alias': alias}) ?? '';

  Future<bool> existe(String alias) async =>
      await _canal.invokeMethod<bool>('existe', {'alias': alias}) ?? false;

  Future<void> apagar(String alias) =>
      _canal.invokeMethod<void>('apagar', {'alias': alias});
}

class CofreDoAparelho implements Cofre {
  CofreDoAparelho({
    required this.pasta,
    required this.chave,
    this.alias = aliasDaIdentidade,
  });

  final String pasta;
  final String alias;
  final ChaveDoAparelho chave;

  @override
  bool existe() => nucleo.cofreExiste(pasta: pasta);

  @override
  int esperaSegundos() =>
      _traduzir(() => nucleo.cofreEsperaSegundos(pasta: pasta));

  @override
  Future<void> criar(List<int> pin) async {
    final digitos = _digitos(pin);
    Uint8List? esticado;
    Uint8List? chaveDoCofre;
    try {
      await _traduzirAsync(() => chave.criar(alias));
      final p = await _traduzirAsync(
        () => nucleo.cofrePrepararPinNovo(pin: digitos),
      );
      esticado = p.esticado;
      final c = await _traduzirAsync(() => chave.hmac(alias, p.esticado));
      chaveDoCofre = c;
      await _traduzirAsync(
        () => nucleo.cofreCriar(pasta: pasta, sal: p.sal, chave: c),
      );
    } finally {
      _zerar(digitos);
      _zerar(esticado);
      _zerar(chaveDoCofre);
    }
  }

  @override
  Future<String> abrir(List<int> pin) async {
    final digitos = _digitos(pin);
    Uint8List? esticado;
    Uint8List? chaveDoCofre;
    try {
      final e = await _traduzirAsync(
        () => nucleo.cofrePrepararAbertura(pasta: pasta, pin: digitos),
      );
      esticado = e;
      final c = await _traduzirAsync(() => chave.hmac(alias, e));
      chaveDoCofre = c;
      return await _traduzirAsync(
        () => nucleo.cofreAbrir(pasta: pasta, chave: c),
      );
    } finally {
      _zerar(digitos);
      _zerar(esticado);
      _zerar(chaveDoCofre);
    }
  }

  /// Dígitos 0 a 9 viram ASCII ('0' a '9'), como o núcleo espera.
  static Uint8List _digitos(List<int> pin) {
    if (pin.any((d) => d < 0 || d > 9)) {
      throw const FalhaDoCofre('PIN_INVALIDO');
    }
    return Uint8List.fromList([for (final d in pin) 0x30 + d]);
  }

  static void _zerar(Uint8List? b) => b?.fillRange(0, b.length, 0);

  static T _traduzir<T>(T Function() f) {
    try {
      return f();
    } on AnyhowException catch (e) {
      throw _deNucleo(e);
    }
  }

  static Future<T> _traduzirAsync<T>(Future<T> Function() f) async {
    try {
      return await f();
    } on AnyhowException catch (e) {
      throw _deNucleo(e);
    } on PlatformException catch (e) {
      throw FalhaDoCofre(e.code);
    }
  }

  /// O núcleo manda só o código fixo (às vezes seguido do tipo de erro de disco).
  static Exception _deNucleo(AnyhowException e) {
    final codigo = e.message.trim().split(RegExp(r'\s')).first;
    if (codigo == 'PIN_INCORRETO') return const PinIncorreto();
    return FalhaDoCofre(codigo);
  }
}
