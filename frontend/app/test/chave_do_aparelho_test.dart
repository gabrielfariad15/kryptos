// Canal da chave do aparelho no computador, com um canal falso que devolve o que
// o motor do Flutter devolve no Android: bytes somente leitura.

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kryptos/cofre.dart';
import 'package:kryptos/cofre_do_aparelho.dart';

/// Responde ao `hmac` com [resposta] como lista somente leitura, igual ao buffer
/// da mensagem de plataforma no aparelho (achado no teste do Motorola em 07/11/2026).
class CanalFalso extends MethodChannel {
  CanalFalso(this.resposta) : super('kryptos/chave-do-aparelho-falso');

  final Uint8List resposta;

  @override
  Future<T?> invokeMethod<T>(String method, [dynamic arguments]) async {
    if (method != 'hmac') throw MissingPluginException(method);
    return resposta.asUnmodifiableView() as T;
  }
}

void main() {
  test(
    'hmac devolve cópia que pode ser zerada, mesmo vindo somente leitura',
    () async {
      final original = Uint8List.fromList(List.generate(32, (i) => i + 1));
      final chave = ChaveDoAparelho(CanalFalso(original));
      final r = await chave.hmac('alias', Uint8List(32));
      expect(r, original);
      r.fillRange(0, r.length, 0);
      expect(r, everyElement(0));
    },
  );

  test('hmac com tamanho diferente de 32 bytes é recusado', () async {
    for (final n in [0, 31, 33]) {
      final chave = ChaveDoAparelho(CanalFalso(Uint8List(n)));
      await expectLater(
        chave.hmac('alias', Uint8List(32)),
        throwsA(
          isA<FalhaDoCofre>().having(
            (e) => e.codigo,
            'codigo',
            'HMAC_INVALIDO',
          ),
        ),
      );
    }
  });
}
