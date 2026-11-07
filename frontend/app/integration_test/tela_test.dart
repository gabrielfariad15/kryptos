// Teste no aparelho da tela segura: a janela do app tem FLAG_SECURE (sem print, sem
// gravação de tela, sem miniatura nos recentes). Confere na janela de verdade, pelo
// canal kryptos/tela (MainActivity.kt).
//
// O que este teste não prova: que o sistema respeita a proteção. Isso se confere
// tentando um print no aparelho (fica preto ou o Android avisa que o app não deixa).
//
// Imprime uma linha MEDIDA. Nada de texto, chave ou código.

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  test('a janela do app tem FLAG_SECURE', () async {
    const canal = MethodChannel('kryptos/tela');
    final segura = await canal.invokeMethod<bool>('segura');
    expect(segura, isTrue);
    // ignore: avoid_print
    print('MEDIDA tela: FLAG_SECURE ligado');
  });
}
