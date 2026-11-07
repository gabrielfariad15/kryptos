// Testes do desbloqueio e da escolha da tela de entrada, no computador, com o cofre
// falso de apoio.dart.

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'apoio.dart';

CofreFalso comPin() => CofreFalso(gravado: [1, 3, 5, 7, 9, 0]);

void main() {
  setUpAll(carregarFontes);

  testWidgets('sem cofre gravado, o app abre na primeira abertura', (
    tester,
  ) async {
    await abrir(tester, cofre: CofreFalso());
    expect(find.text('PRIMEIRA ABERTURA'), findsOneWidget);
    expect(find.text('DESBLOQUEIO'), findsNothing);
  });

  testWidgets('com cofre gravado, o app abre no desbloqueio', (tester) async {
    await abrir(tester, cofre: comPin());
    expect(find.text('DESBLOQUEIO'), findsOneWidget);
    expect(find.text('PRIMEIRA ABERTURA'), findsNothing);
    expect(find.byKey(const Key('botao-GERAR CÓDIGO')), findsNothing);
  });

  testWidgets('ABRIR só liga com os seis dígitos', (tester) async {
    await abrir(tester, cofre: comPin());
    expect(acaoDoBotao(tester, 'ABRIR'), isNull);
    await digitar(tester, '13579');
    expect(acaoDoBotao(tester, 'ABRIR'), isNull);
    await tocar(tester, '0');
    expect(acaoDoBotao(tester, 'ABRIR'), isNotNull);
  });

  testWidgets('PIN certo mostra o cofre aberto com o código', (tester) async {
    final cofre = comPin();
    await abrir(tester, cofre: cofre);
    await digitar(tester, '135790');
    await tocarBotao(tester, 'ABRIR');
    await tester.pumpAndSettle();
    expect(find.text('COFRE ABERTO'), findsOneWidget);
    expect(find.text(codigoFalso), findsOneWidget);
    expect(cofre.ultimaListaRecebida, isEmpty, reason: 'PIN não foi zerado');
  });

  testWidgets('PIN errado não abre, avisa e limpa as casas', (tester) async {
    final cofre = comPin();
    await abrir(tester, cofre: cofre);
    await digitar(tester, '135791');
    await tocarBotao(tester, 'ABRIR');
    await tester.pumpAndSettle();
    expect(find.text('DESBLOQUEIO'), findsOneWidget);
    expect(find.text(codigoFalso), findsNothing);
    expect(textoDoAviso(tester), 'PIN incorreto.');
    expect(pontos(), 0);
    expect(cofre.ultimaListaRecebida, isEmpty);

    // E dá para tentar de novo com o certo.
    await digitar(tester, '135790');
    await tocarBotao(tester, 'ABRIR');
    await tester.pumpAndSettle();
    expect(find.text('COFRE ABERTO'), findsOneWidget);
  });

  testWidgets('dígitos do PIN não aparecem escritos na tela', (tester) async {
    await abrir(tester, cofre: comPin());
    await digitar(tester, '77');
    expect(find.text('7'), findsOneWidget, reason: 'só a tecla');
  });

  testWidgets('com espera, avisa e desliga o teclado durante a conferência', (
    tester,
  ) async {
    final cofre = comPin()
      ..espera = 30
      ..segurar = Completer<void>();
    await abrir(tester, cofre: cofre);
    await digitar(tester, '135790');
    await tocarBotao(tester, 'ABRIR');
    expect(textoDoAviso(tester), contains('30 s'));
    expect(find.text('CONFERINDO…'), findsOneWidget);
    expect(acaoDoBotao(tester, 'ABRIR'), isNull);
    expect(
      tester.widget<InkWell>(find.byKey(const Key('tecla-1'))).onTap,
      isNull,
    );
    cofre.segurar!.complete();
    await tester.pumpAndSettle();
    expect(find.text('COFRE ABERTO'), findsOneWidget);
  });

  testWidgets('chave do aparelho apagada: aviso próprio, sem quebrar', (
    tester,
  ) async {
    final cofre = comPin()..falhaAoAbrir = 'CHAVE_AUSENTE';
    await abrir(tester, cofre: cofre);
    await digitar(tester, '135790');
    await tocarBotao(tester, 'ABRIR');
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
    expect(textoDoAviso(tester), contains('não existe mais'));
  });

  testWidgets('outra falha mostra o código do erro', (tester) async {
    final cofre = comPin()..falhaAoAbrir = 'COFRE_CORROMPIDO';
    await abrir(tester, cofre: cofre);
    await digitar(tester, '135790');
    await tocarBotao(tester, 'ABRIR');
    await tester.pumpAndSettle();
    expect(textoDoAviso(tester), contains('COFRE_CORROMPIDO'));
    expect(acaoDoBotao(tester, 'ABRIR'), isNull, reason: 'casas limpas');
  });

  testWidgets('no motorola o desbloqueio cabe sem rolar, com aviso', (
    tester,
  ) async {
    await abrir(tester, cofre: comPin());
    conferirQueCabe(tester, 'ABRIR');
    await digitar(tester, '000000');
    await tocarBotao(tester, 'ABRIR');
    await tester.pumpAndSettle();
    expect(textoDoAviso(tester), isNotNull);
    conferirQueCabe(tester, 'ABRIR');
  });

  testWidgets('fonte de 160% não estoura desbloqueio nem cofre aberto', (
    tester,
  ) async {
    tester.platformDispatcher.textScaleFactorTestValue = 1.6;
    addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
    await abrir(tester, cofre: comPin());
    expect(tester.takeException(), isNull);
    await digitar(tester, '135790');
    await tocarBotao(tester, 'ABRIR');
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
    expect(find.text(codigoFalso), findsOneWidget);
  });

  testWidgets('alvos de toque do desbloqueio com 48 dp e rótulo', (
    tester,
  ) async {
    await abrir(tester, cofre: comPin());
    final semantica = tester.ensureSemantics();
    await expectLater(tester, meetsGuideline(androidTapTargetGuideline));
    await expectLater(tester, meetsGuideline(labeledTapTargetGuideline));
    semantica.dispose();
  });
}
