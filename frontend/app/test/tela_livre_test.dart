// A faixa da tela liberada (build de teste sem FLAG_SECURE): aparece só quando o app é
// montado com telaLivre, por cima das telas, sem pegar toque.

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kryptos/tela_inicial.dart';

import 'apoio.dart';

Future<void> montar(WidgetTester tester, {required bool telaLivre}) async {
  await tester.pumpWidget(
    KryptosApp(
      versaoNucleo: '9.8.7',
      gerarCodigo: gerarFalso,
      cofre: CofreFalso(),
      agenda: AgendaFalsa(),
      abrirCamera: FabricaDeCameras().call,
      mensageiro: MensageiroFalso(),
      telaLivre: telaLivre,
    ),
  );
}

void main() {
  setUpAll(carregarFontes);

  testWidgets('app normal: sem faixa', (tester) async {
    await montar(tester, telaLivre: false);
    expect(find.byKey(const Key('faixa-tela-livre')), findsNothing);
  });

  testWidgets('tela livre: faixa por cima e os toques passam', (tester) async {
    await montar(tester, telaLivre: true);
    expect(find.byKey(const Key('faixa-tela-livre')), findsOneWidget);
    expect(
      find.text('TELA LIBERADA · SÓ TESTE · PRINT PERMITIDO'),
      findsOneWidget,
    );
    expect(
      find.ancestor(
        of: find.byKey(const Key('faixa-tela-livre')),
        matching: find.byType(IgnorePointer),
      ),
      findsWidgets,
    );
    expect(tester.takeException(), isNull);
  });
}
