// Teste no aparelho, com o núcleo em Rust de verdade: prova que a ponte
// carrega a biblioteca e que a versão chega à tela.
//
// Rodar (com o celular no cabo):
//   flutter test integration_test

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:kryptos/src/rust/api/nucleo.dart';
import 'package:kryptos/src/rust/frb_generated.dart';
import 'package:kryptos/tela_inicial.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async => await RustLib.init());

  testWidgets('núcleo responde pela ponte e a versão aparece na tela', (
    tester,
  ) async {
    final versao = versaoNucleo();
    expect(versao, '0.1.0');
    await tester.pumpWidget(KryptosApp(versaoNucleo: versao));
    expect(find.text('NÚCLEO v0.1.0'), findsOneWidget);
  });
}
