// Testes da tela no computador, sem o núcleo em Rust (a versão é injetada).

import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kryptos/tela_inicial.dart';

/// Tela de um aparelho: tamanho físico, densidade e barras do sistema (em pixels).
class Aparelho {
  const Aparelho(
    this.nome,
    this.largura,
    this.altura,
    this.densidade,
    this.barraTopo,
    this.barraBase,
  );
  final String nome;
  final double largura, altura, densidade, barraTopo, barraBase;
}

// Medido na captura de 06/11/2026 (1080x2400): casa do PIN de 52 dp = 130 px,
// logo densidade 2,5; barra de status 110 px; barra de 3 botões 120 px.
const motorolaEdge40Neo = Aparelho(
  'motorola edge 40 neo',
  1080,
  2400,
  2.5,
  110,
  120,
);
// Celular pequeno comum (360 x 740 dp), para provar que nada some quando não cabe.
const pequeno = Aparelho('pequeno 360x740', 1080, 2220, 3.0, 72, 144);

Future<void> abrir(
  WidgetTester tester, {
  Aparelho aparelho = motorolaEdge40Neo,
}) async {
  tester.view.physicalSize = Size(aparelho.largura, aparelho.altura);
  tester.view.devicePixelRatio = aparelho.densidade;
  tester.view.padding = FakeViewPadding(
    top: aparelho.barraTopo,
    bottom: aparelho.barraBase,
  );
  tester.view.viewPadding = FakeViewPadding(
    top: aparelho.barraTopo,
    bottom: aparelho.barraBase,
  );
  addTearDown(tester.view.reset);
  await tester.pumpWidget(const KryptosApp(versaoNucleo: '9.8.7'));
}

/// Linha de base do texto na tela: topo do parágrafo + distância até a base,
/// medida com o mesmo texto e a mesma escala que o parágrafo usou.
double linhaDeBase(WidgetTester tester, String texto) {
  final finder = find.text(texto);
  final paragrafo = tester.renderObject<RenderParagraph>(finder);
  final pintor = TextPainter(
    text: paragrafo.text,
    textDirection: TextDirection.ltr,
    textScaler: paragrafo.textScaler,
  )..layout();
  final base = pintor.computeDistanceToActualBaseline(TextBaseline.alphabetic);
  pintor.dispose();
  return tester.getTopLeft(finder).dy + base;
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
