// Apoio comum dos testes de tela no computador, sem o núcleo em Rust e sem a câmera:
// aparelhos medidos, fonte de verdade, cofre, agenda e câmera falsos e atalhos de
// toque.

import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kryptos/cofre.dart';
import 'package:kryptos/contatos.dart';
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

/// Código do vetor conhecido do núcleo (nucleo/src/codigo.rs), usado como falso.
const codigoFalso = '2J2F-P63Y';

String gerarFalso() => codigoFalso;

/// Cofre em memória. Guarda cópias do que recebeu e a última lista entregue pela
/// tela, para conferir que ela foi zerada depois.
class CofreFalso implements Cofre {
  CofreFalso({this.gravado, this.espera = 0});

  List<int>? gravado;
  int espera;
  String? falhaAoCriar;
  String? falhaAoAbrir;

  /// Se definido, `criar` e `abrir` só terminam quando ele completar.
  Completer<void>? segurar;

  final criados = <List<int>>[];
  final tentativas = <List<int>>[];
  List<int>? ultimaListaRecebida;

  @override
  bool existe() => gravado != null;

  @override
  int esperaSegundos() => espera;

  @override
  Future<void> criar(List<int> pin) async {
    ultimaListaRecebida = pin;
    criados.add(List.of(pin));
    await segurar?.future;
    final f = falhaAoCriar;
    if (f != null) throw FalhaDoCofre(f);
    gravado = List.of(pin);
  }

  @override
  Future<String> abrir(List<int> pin) async {
    ultimaListaRecebida = pin;
    tentativas.add(List.of(pin));
    await segurar?.future;
    final f = falhaAoAbrir;
    if (f != null) throw FalhaDoCofre(f);
    if (gravado == null) throw const FalhaDoCofre('COFRE_NAO_EXISTE');
    if (!listEquals(pin, gravado)) throw const PinIncorreto();
    return codigoFalso;
  }
}

/// Grade de mentira com o tamanho da de verdade (37 módulos, margem 4): um xadrez.
GradeQr gradeFalsa() => GradeQr(
  lado: 37,
  margem: 4,
  modulos: Uint8List.fromList([
    for (var y = 0; y < 37; y++)
      for (var x = 0; x < 37; x++) (x + y).isEven ? 1 : 0,
  ]),
);

/// Agenda em memória. [responder] decide o que cada quadro devolve.
class AgendaFalsa implements Agenda {
  AgendaFalsa({GradeQr? grade, this.semIdentidade = false})
    : _grade = grade ?? gradeFalsa();

  final GradeQr _grade;
  final bool semIdentidade;
  final lista = <ContatoNaAgenda>[];
  final quadros = <Quadro>[];

  /// Padrão: nenhum QR no quadro.
  Future<ContatoLido?> Function(Quadro q) responder = (_) async => null;

  @override
  GradeQr? gradePropria() => semIdentidade ? null : _grade;

  @override
  Future<ContatoLido?> lerQuadro(Quadro quadro) async {
    quadros.add(quadro);
    final r = await responder(quadro);
    if (r != null && r.novo) {
      lista.add(ContatoNaAgenda(codigo: r.codigo, conferidoPorQr: true));
    }
    return r;
  }

  @override
  List<ContatoNaAgenda> contatos() => List.of(lista);
}

/// Câmera que não liga nada: o teste manda os quadros com [mandar].
class CameraFalsa implements Camera {
  String? falhaAoAbrir;

  /// Se definido, `abrir` só termina quando ele completar.
  Completer<void>? segurarAbertura;
  bool aberta = false;
  int fechamentos = 0;
  void Function(Quadro q)? _aoQuadro;

  bool get recebendo => _aoQuadro != null;

  @override
  Future<void> abrir() async {
    await segurarAbertura?.future;
    final f = falhaAoAbrir;
    if (f != null) throw FalhaDaCamera(f);
    aberta = true;
  }

  @override
  Widget previa() =>
      const ColoredBox(key: Key('previa-falsa'), color: Color(0xFF223344));

  @override
  Future<void> receberQuadros(void Function(Quadro q) aoQuadro) async {
    _aoQuadro = aoQuadro;
  }

  void mandar([Quadro? q]) => _aoQuadro?.call(
    q ??
        Quadro(
          brilho: Uint8List(640 * 480),
          largura: 640,
          altura: 480,
          passo: 640,
        ),
  );

  @override
  Future<void> fechar() async {
    fechamentos++;
    aberta = false;
    _aoQuadro = null;
  }
}

/// Fábrica que guarda as câmeras que a tela abriu.
class FabricaDeCameras {
  final abertas = <CameraFalsa>[];
  void Function(CameraFalsa c)? preparar;

  CameraFalsa call() {
    final c = CameraFalsa();
    preparar?.call(c);
    abertas.add(c);
    return c;
  }

  CameraFalsa get ultima => abertas.last;
}

Future<void> abrir(
  WidgetTester tester, {
  Aparelho aparelho = motorolaEdge40Neo,
  String Function() gerarCodigo = gerarFalso,
  Cofre? cofre,
  Agenda? agenda,
  AbrirCamera? abrirCamera,
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
  await tester.pumpWidget(
    KryptosApp(
      versaoNucleo: '9.8.7',
      gerarCodigo: gerarCodigo,
      cofre: cofre ?? CofreFalso(),
      agenda: agenda ?? AgendaFalsa(),
      abrirCamera: abrirCamera ?? FabricaDeCameras().call,
    ),
  );
}

Future<void> tocarBotao(WidgetTester tester, String nome) async {
  final alvo = find.byKey(Key('botao-$nome'));
  await tester.ensureVisible(alvo);
  await tester.tap(alvo);
  await tester.pump();
}

VoidCallback? acaoDoBotao(WidgetTester tester, String nome) =>
    tester.widget<InkWell>(find.byKey(Key('botao-$nome'))).onTap;

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

Future<void> digitar(WidgetTester tester, String digitos) async {
  for (final d in digitos.split('')) {
    await tocar(tester, d);
  }
}

int pontos() => find
    .byWidgetPredicate(
      (w) =>
          w.key is ValueKey<String> &&
          (w.key as ValueKey<String>).value.startsWith('ponto-'),
    )
    .evaluate()
    .length;

String? textoDoAviso(WidgetTester tester) {
  final f = find.byKey(const Key('aviso'));
  if (f.evaluate().isEmpty) return null;
  return tester.widget<Text>(f).data;
}

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

/// O conteúdo rolável cabe inteiro na tela e o botão de ação fica acima do rodapé.
void conferirQueCabe(WidgetTester tester, String botao) {
  final rolagem = tester
      .state<ScrollableState>(find.byType(Scrollable))
      .position;
  expect(
    rolagem.maxScrollExtent,
    0,
    reason: 'conteúdo passa da tela em ${rolagem.maxScrollExtent} dp',
  );
  final acao = tester.getRect(find.byKey(Key('botao-$botao')));
  final rodape = tester.getRect(find.byKey(const Key('rodape')));
  expect(acao.bottom, lessThanOrEqualTo(rodape.top));
}
