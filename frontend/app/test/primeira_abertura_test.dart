// Testes da tela no computador, sem o núcleo em Rust (a versão é injetada).

import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kryptos/tela_inicial.dart';

Future<void> abrir(WidgetTester tester) async {
  tester.view.physicalSize = const Size(1220, 2712);
  tester.view.devicePixelRatio = 3.0;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(const KryptosApp(versaoNucleo: '9.8.7'));
}

Future<void> tocar(WidgetTester tester, String rotulo) async {
  final alvo = find.byKey(Key('tecla-$rotulo'));
  await tester.ensureVisible(alvo);
  await tester.tap(alvo);
  await tester.pump();
}

int pontos() => find
    .byWidgetPredicate(
      (w) =>
          w.key is ValueKey<String> &&
          (w.key as ValueKey<String>).value.startsWith('ponto-'),
    )
    .evaluate()
    .length;

/// Carrega a IBM Plex Mono de verdade: a fonte padrão dos testes tem letras
/// mais largas e daria estouro de layout que não existe no aparelho.
Future<void> carregarFontes() async {
  final carregador = FontLoader('IBMPlexMono');
  for (final peso in ['Regular', 'Medium', 'SemiBold', 'Bold']) {
    final bytes = File('assets/fonts/IBMPlexMono-$peso.ttf').readAsBytesSync();
    carregador.addFont(Future.value(ByteData.sublistView(bytes)));
  }
  await carregador.load();
}

void main() {
  setUpAll(carregarFontes);

  testWidgets('fonte grande do sistema (160%) não estoura o layout', (
    tester,
  ) async {
    tester.platformDispatcher.textScaleFactorTestValue = 1.6;
    addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
    await abrir(tester);
    expect(tester.takeException(), isNull);
    await tocar(tester, '5');
    expect(pontos(), 1);
  });

  testWidgets('mostra a versão que veio do núcleo', (tester) async {
    await abrir(tester);
    expect(find.text('NÚCLEO v9.8.7'), findsOneWidget);
    expect(find.text('KRYPTOS'), findsOneWidget);
  });

  testWidgets('teclado preenche e apaga as casas do PIN', (tester) async {
    await abrir(tester);
    expect(pontos(), 0);
    await tocar(tester, '1');
    await tocar(tester, '2');
    await tocar(tester, '3');
    expect(pontos(), 3);
    await tocar(tester, 'APAGAR');
    expect(pontos(), 2);
  });

  testWidgets('PIN não passa de seis dígitos', (tester) async {
    await abrir(tester);
    for (final d in ['1', '2', '3', '4', '5', '6', '7', '8']) {
      await tocar(tester, d);
    }
    expect(pontos(), 6);
  });

  testWidgets('apagar com PIN vazio não quebra', (tester) async {
    await abrir(tester);
    await tocar(tester, 'APAGAR');
    expect(pontos(), 0);
  });

  testWidgets('dígitos do PIN não aparecem escritos na tela', (tester) async {
    await abrir(tester);
    await tocar(tester, '7');
    await tocar(tester, '7');
    // O único "7" na tela é a própria tecla.
    expect(find.text('7'), findsOneWidget);
  });

  testWidgets('GERAR CÓDIGO e CONTINUAR ficam desligados nesta etapa', (
    tester,
  ) async {
    await abrir(tester);
    for (final rotulo in ['GERAR CÓDIGO', 'CONTINUAR']) {
      final botao = tester.widget<InkWell>(find.byKey(Key('botao-$rotulo')));
      expect(botao.onTap, isNull, reason: '$rotulo deveria estar desligado');
    }
    expect(find.text('····-····'), findsOneWidget);
  });
}
