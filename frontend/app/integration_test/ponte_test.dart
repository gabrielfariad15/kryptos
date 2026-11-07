// Teste no aparelho, com o núcleo em Rust de verdade: prova que a ponte
// carrega a biblioteca, que a versão chega à tela e que GERAR CÓDIGO cria a
// identidade no núcleo e mostra o código de contato.
//
// Rodar (com o celular no cabo):
//   flutter test integration_test

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:kryptos/agenda_do_aparelho.dart';
import 'package:kryptos/camera_do_aparelho.dart';
import 'package:kryptos/cofre_do_aparelho.dart';
import 'package:kryptos/src/rust/api/nucleo.dart';
import 'package:kryptos/src/rust/frb_generated.dart';
import 'package:kryptos/tela_inicial.dart';

/// XXXX-XXXX no alfabeto Crockford (sem I, L, O e U), como sai do núcleo.
final formatoCodigo = RegExp(r'^[0-9A-HJKMNP-TV-Z]{4}-[0-9A-HJKMNP-TV-Z]{4}$');

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  late CofreDoAparelho cofreVazio;

  setUpAll(() async {
    await RustLib.init();
    final chave = ChaveDoAparelho();
    final pastas = await chave.pastas();
    // Pasta própria e vazia: a tela abre na primeira abertura sem tocar no cofre
    // de verdade do app.
    final pasta = '${pastas.semBackup}/teste-ponte';
    await cofreApagar(pasta: pasta);
    cofreVazio = CofreDoAparelho(
      pasta: pasta,
      chave: chave,
      alias: 'kryptos-teste-ponte',
    );
  });

  setUp(esquecerIdentidade);
  tearDown(esquecerIdentidade);

  testWidgets('núcleo responde pela ponte e a versão aparece na tela', (
    tester,
  ) async {
    final versao = versaoNucleo();
    expect(versao, '0.1.0');
    await tester.pumpWidget(
      KryptosApp(
        versaoNucleo: versao,
        gerarCodigo: gerarCodigoContato,
        cofre: cofreVazio,
        agenda: const AgendaDoAparelho(),
        abrirCamera: CameraDoAparelho.new,
      ),
    );
    expect(find.text('NÚCLEO v0.1.0'), findsOneWidget);
  });

  testWidgets('GERAR CÓDIGO cria a identidade no núcleo e mostra o código', (
    tester,
  ) async {
    await tester.pumpWidget(
      KryptosApp(
        versaoNucleo: versaoNucleo(),
        gerarCodigo: gerarCodigoContato,
        cofre: cofreVazio,
        agenda: const AgendaDoAparelho(),
        abrirCamera: CameraDoAparelho.new,
      ),
    );
    expect(find.text('····-····'), findsOneWidget);

    final botao = find.byKey(const Key('botao-GERAR CÓDIGO'));
    await tester.ensureVisible(botao);
    await tester.tap(botao);
    await tester.pump();

    final naTela = tester.widget<Text>(find.byKey(const Key('codigo'))).data;
    expect(naTela, matches(formatoCodigo));
    // Nova chamada pela ponte devolve o mesmo código: a identidade é uma só.
    expect(gerarCodigoContato(), naTela);
    expect(tester.takeException(), isNull);
  });
}
