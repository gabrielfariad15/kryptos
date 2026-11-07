// Teste no aparelho da caixa selada, com o núcleo em Rust e o Keystore de verdade.
// Pasta e aliases próprios: não toca no cofre nem nas gavetas do app.
//
// Prova, num aparelho só (A e B são duas identidades, uma de cada vez na memória):
//   - B cria o cofre (agenda, caixa com 100 gavetas e identidade) e as 100 chaves de
//     gaveta no Keystore; publica uma chave avulsa;
//   - A abre a conversa com a chave avulsa de B e cifra uma carta;
//   - B reabre, recebe e sela a carta; a mesma carta de novo é reconhecida;
//   - antes de ler, o texto não está em arquivo nenhum do app;
//   - B lê (remetente e texto certos) e queima; a chave da gaveta é trocada;
//   - depois da queima, o texto não está em arquivo nenhum do app;
//   - a busca é calibrada no próprio aparelho: o texto gravado em claro é achado.
//
// Imprime uma linha MEDIDA com os tempos. Nada de texto, chave ou código.
//
// Rodar (com o celular no cabo):
//   flutter test integration_test

import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:kryptos/caixa_do_aparelho.dart';
import 'package:kryptos/cofre_do_aparelho.dart';
import 'package:kryptos/src/rust/api/nucleo.dart';
import 'package:kryptos/src/rust/frb_generated.dart';

const aliasDoCofre = 'kryptos-teste-caixa';
const prefixoDeTeste = 'kryptos-teste-gaveta-';
const pin = [2, 4, 6, 8, 0, 1];

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  late ChaveDoAparelho chave;
  late PastasDoApp pastas;
  late String pastaB;
  late CofreDoAparelho cofre;
  late CaixaDoAparelho caixa;

  Future<void> limpar() async {
    esquecerIdentidade();
    await cofreApagar(pasta: pastaB);
    await chave.apagar(aliasDoCofre);
    for (var g = 0; g < capacidadeDaCaixa; g++) {
      await chave.apagar(caixa.alias(g));
    }
    final vazamento = File('$pastaB/vazamento');
    if (vazamento.existsSync()) vazamento.deleteSync();
  }

  setUpAll(() async {
    await RustLib.init();
    chave = ChaveDoAparelho();
    pastas = await chave.pastas();
    pastaB = '${pastas.semBackup}/teste-caixa';
    cofre = CofreDoAparelho(pasta: pastaB, chave: chave, alias: aliasDoCofre);
    caixa = CaixaDoAparelho(chave: chave, prefixo: prefixoDeTeste);
  });

  setUp(limpar);
  tearDown(limpar);

  // Prazo de 10 min: cria 100 chaves no Keystore, grava 6,5 MB de gavetas e varre a
  // pasta do app duas vezes, com o núcleo compilado sem otimização no build de teste.
  test(
    'do recebimento à queima, sem o texto no disco',
    timeout: const Timeout(Duration(minutes: 10)),
    () async {
      final relogio = Stopwatch()..start();

      // B: cofre, chaves das gavetas, cartão e chave avulsa.
      gerarCodigoContato();
      await cofre.criar(List.of(pin));
      final msCofre = relogio.elapsedMilliseconds;
      relogio.reset();
      expect(await caixa.prepararChaves(), capacidadeDaCaixa);
      final msChaves = relogio.elapsedMilliseconds;
      expect(await caixa.prepararChaves(), 0, reason: 'criou chave que já existia');
      final nivel = await chave.nivel(caixa.alias(0));
      expect(nivel, anyOf('STRONGBOX', 'TEE', 'HARDWARE'));
      final cartaoB = cartaoProprioTexto();
      expect(cartaoB, isNotNull);
      final avulsa = await novaChaveAvulsa();
      esquecerIdentidade();

      // A, sem cofre: conversa com a chave avulsa de B e uma carta.
      final codigoA = gerarCodigoContato();
      final b = await adicionarContatoPorTexto(texto: cartaoB!);
      await abrirConversa(codigo: b.codigo, avulsa: avulsa);
      final marca = DateTime.now().microsecondsSinceEpoch;
      final texto = utf8.encode('MENSAGEM-NO-APARELHO-ação-$marca');
      final carta = await cifrarCarta(codigo: b.codigo, texto: texto);
      final cartaoA = cartaoProprioTexto();
      expect(cartaoA, isNotNull);
      esquecerIdentidade();

      // B reabre o app.
      await cofre.abrir(List.of(pin));
      relogio.reset();
      await adicionarContatoPorTexto(texto: cartaoA!);
      final msAgenda = relogio.elapsedMilliseconds;
      expect(await caixa.queimarLidas(), 0);

      relogio.reset();
      expect(await caixa.receber(carta), Chegada.selada);
      final msSelar = relogio.elapsedMilliseconds;
      expect(await caixa.receber(carta), Chegada.jaGuardada);
      final lista = await caixa.listar();
      expect(lista, hasLength(1));
      expect(lista.first.lida, isFalse);
      final antes = await procurarTexto(pasta: pastas.dados, texto: texto);
      expect(antes.ocorrencias, 0, reason: 'texto em claro antes de ler');

      relogio.reset();
      final lida = await caixa.ler(lista.first.gaveta);
      final msLer = relogio.elapsedMilliseconds;
      expect(lida.remetente, codigoA);
      expect(lida.texto, texto);
      expect((await caixa.listar()).first.lida, isTrue);

      relogio.reset();
      expect(await caixa.queimarLidas(), 1);
      final msQueimar = relogio.elapsedMilliseconds;
      expect(await caixa.listar(), isEmpty);

      relogio.reset();
      final depois = await procurarTexto(pasta: pastas.dados, texto: texto);
      final msBusca = relogio.elapsedMilliseconds;
      expect(depois.ocorrencias, 0, reason: 'texto em claro depois da queima');
      expect(depois.ilegiveis, 0);
      expect(depois.arquivos, greaterThan(capacidadeDaCaixa));

      // Calibração da busca no próprio aparelho: o texto gravado em claro é achado.
      File('$pastaB/vazamento').writeAsBytesSync(texto, flush: true);
      final calibracao = await procurarTexto(pasta: pastas.dados, texto: texto);
      expect(calibracao.ocorrencias, greaterThanOrEqualTo(1));

      // ignore: avoid_print
      print(
        'MEDIDA caixa: cofre=${msCofre}ms chaves=${msChaves}ms ($nivel) agenda=${msAgenda}ms '
        'selar=${msSelar}ms ler=${msLer}ms queimar=${msQueimar}ms '
        'busca=${msBusca}ms em ${depois.arquivos} arquivos',
      );
    },
  );
}
