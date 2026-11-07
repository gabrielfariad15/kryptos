// Testes da primeira abertura no computador, sem o núcleo em Rust (a versão, o
// gerador do código e o cofre são injetados; ver apoio.dart).

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'apoio.dart';

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

  testWidgets('no motorola cabe tudo sem rolar e o CONTINUAR fica inteiro', (
    tester,
  ) async {
    await abrir(tester);
    final rolagem = tester
        .state<ScrollableState>(find.byType(Scrollable))
        .position;
    expect(
      rolagem.maxScrollExtent,
      0,
      reason: 'conteúdo passa da tela em ${rolagem.maxScrollExtent} dp',
    );
    final continuar = tester.getRect(find.byKey(const Key('botao-CONTINUAR')));
    final rodape = tester.getRect(find.byKey(const Key('rodape')));
    expect(continuar.bottom, lessThanOrEqualTo(rodape.top));
  });

  testWidgets(
    'em celular pequeno o CONTINUAR continua inteiro e o resto rola',
    (tester) async {
      await abrir(tester, aparelho: pequeno);
      final continuar = tester.getRect(
        find.byKey(const Key('botao-CONTINUAR')),
      );
      final rodape = tester.getRect(find.byKey(const Key('rodape')));
      expect(continuar.bottom, lessThanOrEqualTo(rodape.top));
      expect(continuar.top, greaterThanOrEqualTo(0));
      await tester.ensureVisible(find.text('SEM CÓPIA FORA DAQUI'));
      await tester.pumpAndSettle();
      expect(
        tester.getRect(find.text('SEM CÓPIA FORA DAQUI')).bottom,
        lessThanOrEqualTo(continuar.top),
      );
    },
  );

  testWidgets('KRYPTOS e PRIMEIRA ABERTURA na mesma linha de base', (
    tester,
  ) async {
    await abrir(tester);
    expect(
      linhaDeBase(tester, 'PRIMEIRA ABERTURA'),
      closeTo(linhaDeBase(tester, 'KRYPTOS'), 0.5),
    );
  });

  testWidgets(
    'barra de navegação escura, com ícones claros e sem véu do sistema',
    (tester) async {
      await abrir(tester);
      final regiao = tester.widget<AnnotatedRegion<SystemUiOverlayStyle>>(
        find
            .byWidgetPredicate(
              (w) => w is AnnotatedRegion<SystemUiOverlayStyle>,
            )
            .first,
      );
      expect(regiao.value.systemNavigationBarContrastEnforced, isFalse);
      expect(regiao.value.systemNavigationBarIconBrightness, Brightness.light);
      expect(regiao.value.statusBarIconBrightness, Brightness.light);
    },
  );

  testWidgets('alvos de toque com pelo menos 48 dp e com rótulo', (
    tester,
  ) async {
    await abrir(tester);
    final semantica = tester.ensureSemantics();
    await expectLater(tester, meetsGuideline(androidTapTargetGuideline));
    await expectLater(tester, meetsGuideline(labeledTapTargetGuideline));
    semantica.dispose();
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

  testWidgets(
    'abre com GERAR CÓDIGO ligado, CONTINUAR desligado e sem código',
    (tester) async {
      await abrir(tester);
      expect(acaoDoBotao(tester, 'GERAR CÓDIGO'), isNotNull);
      expect(acaoDoBotao(tester, 'CONTINUAR'), isNull);
      expect(find.text('····-····'), findsOneWidget);
    },
  );

  testWidgets('GERAR CÓDIGO mostra o código no lugar dos pontos', (
    tester,
  ) async {
    await abrir(tester);
    await tocarBotao(tester, 'GERAR CÓDIGO');
    expect(find.text(codigoFalso), findsOneWidget);
    expect(find.text('····-····'), findsNothing);
    expect(acaoDoBotao(tester, 'GERAR CÓDIGO'), isNull);
    expect(acaoDoBotao(tester, 'CONTINUAR'), isNull);
  });

  testWidgets('código gerado não estoura o layout com fonte de 160%', (
    tester,
  ) async {
    tester.platformDispatcher.textScaleFactorTestValue = 1.6;
    addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
    await abrir(tester);
    await tocarBotao(tester, 'GERAR CÓDIGO');
    expect(tester.takeException(), isNull);
    expect(find.text(codigoFalso), findsOneWidget);
  });

  testWidgets('a identidade é gerada uma vez só, mesmo tocando de novo', (
    tester,
  ) async {
    var chamadas = 0;
    await abrir(
      tester,
      gerarCodigo: () {
        chamadas++;
        return codigoFalso;
      },
    );
    await tocarBotao(tester, 'GERAR CÓDIGO');
    await tocarBotao(tester, 'GERAR CÓDIGO');
    expect(chamadas, 1);
  });

  testWidgets(
    'falha do núcleo mostra aviso, não quebra e deixa tentar de novo',
    (tester) async {
      var chamadas = 0;
      await abrir(
        tester,
        gerarCodigo: () {
          chamadas++;
          if (chamadas == 1) throw StateError('núcleo indisponível');
          return codigoFalso;
        },
      );
      await tocarBotao(tester, 'GERAR CÓDIGO');
      expect(tester.takeException(), isNull);
      expect(find.byKey(const Key('falha-codigo')), findsOneWidget);
      expect(find.text('····-····'), findsOneWidget);
      expect(acaoDoBotao(tester, 'GERAR CÓDIGO'), isNotNull);

      await tocarBotao(tester, 'GERAR CÓDIGO');
      expect(find.byKey(const Key('falha-codigo')), findsNothing);
      expect(find.text(codigoFalso), findsOneWidget);
    },
  );

  // ---------- CONTINUAR e o cofre ----------

  testWidgets('CONTINUAR só liga com o código gerado e os seis dígitos', (
    tester,
  ) async {
    await abrir(tester);
    await digitar(tester, '135790');
    expect(
      acaoDoBotao(tester, 'CONTINUAR'),
      isNull,
      reason: 'sem código ainda',
    );
    await tocarBotao(tester, 'GERAR CÓDIGO');
    expect(acaoDoBotao(tester, 'CONTINUAR'), isNotNull);
    await tocar(tester, 'APAGAR');
    expect(acaoDoBotao(tester, 'CONTINUAR'), isNull, reason: 'cinco dígitos');
  });

  testWidgets('primeiro CONTINUAR pede o PIN de novo, sem gravar', (
    tester,
  ) async {
    final cofre = CofreFalso();
    await abrir(tester, cofre: cofre);
    await tocarBotao(tester, 'GERAR CÓDIGO');
    await digitar(tester, '135790');
    await tocarBotao(tester, 'CONTINUAR');
    expect(find.text('02 · REPITA O PIN'), findsOneWidget);
    expect(pontos(), 0);
    expect(acaoDoBotao(tester, 'CONTINUAR'), isNull);
    expect(cofre.criados, isEmpty);
  });

  testWidgets('PINs diferentes voltam ao começo com aviso e não gravam', (
    tester,
  ) async {
    final cofre = CofreFalso();
    await abrir(tester, cofre: cofre);
    await tocarBotao(tester, 'GERAR CÓDIGO');
    await digitar(tester, '135790');
    await tocarBotao(tester, 'CONTINUAR');
    await digitar(tester, '135791');
    await tocarBotao(tester, 'CONTINUAR');
    expect(find.text('02 · PIN DE ACESSO'), findsOneWidget);
    expect(textoDoAviso(tester), contains('não conferem'));
    expect(pontos(), 0);
    expect(cofre.criados, isEmpty);
  });

  testWidgets('PINs iguais gravam no cofre e abrem o cofre com o código', (
    tester,
  ) async {
    final cofre = CofreFalso();
    await abrir(tester, cofre: cofre);
    await tocarBotao(tester, 'GERAR CÓDIGO');
    await digitar(tester, '135790');
    await tocarBotao(tester, 'CONTINUAR');
    await digitar(tester, '135790');
    await tocarBotao(tester, 'CONTINUAR');
    await tester.pumpAndSettle();
    expect(cofre.criados, [
      [1, 3, 5, 7, 9, 0],
    ]);
    expect(find.text('MODO 01 · CENTRAL'), findsOneWidget);
    expect(find.text(codigoFalso), findsOneWidget);
    // A lista entregue ao cofre foi zerada pela tela depois do uso.
    expect(cofre.ultimaListaRecebida, isEmpty);
  });

  testWidgets('enquanto grava, teclado e CONTINUAR ficam desligados', (
    tester,
  ) async {
    final cofre = CofreFalso()..segurar = Completer<void>();
    await abrir(tester, cofre: cofre);
    await tocarBotao(tester, 'GERAR CÓDIGO');
    await digitar(tester, '135790');
    await tocarBotao(tester, 'CONTINUAR');
    await digitar(tester, '135790');
    await tocarBotao(tester, 'CONTINUAR');
    expect(find.text('GRAVANDO…'), findsOneWidget);
    expect(acaoDoBotao(tester, 'CONTINUAR'), isNull);
    expect(
      tester.widget<InkWell>(find.byKey(const Key('tecla-1'))).onTap,
      isNull,
    );
    cofre.segurar!.complete();
    await tester.pumpAndSettle();
    expect(find.text('MODO 01 · CENTRAL'), findsOneWidget);
  });

  testWidgets('falha ao gravar avisa e deixa tentar de novo', (tester) async {
    final cofre = CofreFalso()..falhaAoCriar = 'DISCO';
    await abrir(tester, cofre: cofre);
    await tocarBotao(tester, 'GERAR CÓDIGO');
    await digitar(tester, '135790');
    await tocarBotao(tester, 'CONTINUAR');
    await digitar(tester, '135790');
    await tocarBotao(tester, 'CONTINUAR');
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
    expect(textoDoAviso(tester), contains('DISCO'));
    expect(find.text('PRIMEIRA ABERTURA'), findsOneWidget);
    expect(acaoDoBotao(tester, 'CONTINUAR'), isNotNull);

    cofre.falhaAoCriar = null;
    await tocarBotao(tester, 'CONTINUAR');
    await tester.pumpAndSettle();
    expect(cofre.criados.length, 2);
    expect(find.text('MODO 01 · CENTRAL'), findsOneWidget);
  });

  testWidgets('na etapa de repetir o PIN, tudo cabe no motorola', (
    tester,
  ) async {
    await abrir(tester);
    await tocarBotao(tester, 'GERAR CÓDIGO');
    await digitar(tester, '135790');
    await tocarBotao(tester, 'CONTINUAR');
    conferirQueCabe(tester, 'CONTINUAR');
  });

  testWidgets('com o aviso de PIN diferente, tudo cabe no motorola', (
    tester,
  ) async {
    await abrir(tester);
    await tocarBotao(tester, 'GERAR CÓDIGO');
    await digitar(tester, '135790');
    await tocarBotao(tester, 'CONTINUAR');
    await digitar(tester, '000000');
    await tocarBotao(tester, 'CONTINUAR');
    expect(textoDoAviso(tester), isNotNull);
    conferirQueCabe(tester, 'CONTINUAR');
  });
}
