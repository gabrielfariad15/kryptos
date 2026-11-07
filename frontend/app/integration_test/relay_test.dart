// Teste no aparelho do caminho pelo relay, com o núcleo em Rust, o Keystore e um relay
// de verdade rodando no computador (a suíte sobe o relay e liga a porta pelo
// `adb reverse`; o endereço chega em --dart-define=KRYPTOS_RELAY_TESTE).
//
// Num aparelho só, duas identidades, uma de cada vez na memória:
//   - B cria o cofre e publica chaves avulsas no relay;
//   - A, sem cofre, lê o cartão de B, retira uma chave de B no relay e manda uma carta;
//   - B reabre, busca no relay, a carta é selada na caixa; B lê o texto certo;
//   - a busca seguinte vem vazia: o relay apagou a carta confirmada.
//
// Imprime uma linha MEDIDA com os tempos. Nada de texto, chave ou código.

import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:kryptos/caixa_do_aparelho.dart';
import 'package:kryptos/cofre_do_aparelho.dart';
import 'package:kryptos/correio.dart';
import 'package:kryptos/relay.dart';
import 'package:kryptos/src/rust/api/nucleo.dart';
import 'package:kryptos/src/rust/frb_generated.dart';

const enderecoDoRelay = String.fromEnvironment('KRYPTOS_RELAY_TESTE');
const aliasDoCofre = 'kryptos-teste-relay';
const prefixoDeTeste = 'kryptos-teste-relay-gaveta-';
const pin = [1, 1, 2, 2, 3, 3];

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  late ChaveDoAparelho chave;
  late String pastaB;
  late CofreDoAparelho cofre;
  late CaixaDoAparelho caixa;
  late ClienteDoRelay relay;
  late Correio correio;

  Future<void> limpar() async {
    esquecerIdentidade();
    await cofreApagar(pasta: pastaB);
    await chave.apagar(aliasDoCofre);
    for (var g = 0; g < capacidadeDaCaixa; g++) {
      await chave.apagar(caixa.alias(g));
    }
  }

  setUpAll(() async {
    await RustLib.init();
    chave = ChaveDoAparelho();
    final pastas = await chave.pastas();
    pastaB = '${pastas.semBackup}/teste-relay';
    cofre = CofreDoAparelho(pasta: pastaB, chave: chave, alias: aliasDoCofre);
    caixa = CaixaDoAparelho(chave: chave, prefixo: prefixoDeTeste);
    if (enderecoDoRelay.isNotEmpty) {
      relay = ClienteDoRelay(Uri.parse(enderecoDoRelay));
      correio = Correio(relay: relay, caixa: caixa);
    }
  });

  setUp(limpar);
  tearDown(limpar);

  test(
    'A manda pelo relay, B busca, sela e lê',
    skip: enderecoDoRelay.isEmpty
        ? 'sem relay: rode pela ferramentas/suite.ps1 -Aparelho'
        : false,
    timeout: const Timeout(Duration(minutes: 10)),
    () async {
      final relogio = Stopwatch()..start();

      // B: cofre, chaves das gavetas e chaves avulsas no relay.
      gerarCodigoContato();
      await cofre.criar(List.of(pin));
      await caixa.prepararChaves();
      relogio.reset();
      expect(await correio.publicarChaves(manter: 5, minimo: 2), 5);
      final msPublicar = relogio.elapsedMilliseconds;
      // Com 5 no relay e mínimo 2, só consulta.
      expect(await correio.publicarChaves(manter: 5, minimo: 2), 5);
      final cartaoB = cartaoProprioTexto();
      expect(cartaoB, isNotNull);
      esquecerIdentidade();

      // A, sem cofre: lê o cartão de B e manda pelo relay.
      final codigoA = gerarCodigoContato();
      final b = await adicionarContatoPorTexto(texto: cartaoB!);
      final texto = utf8.encode('PELO-RELAY-${DateTime.now().microsecondsSinceEpoch}');
      relogio.reset();
      await correio.enviar(b.codigo, texto);
      final msEnviar = relogio.elapsedMilliseconds;
      final cartaoA = cartaoProprioTexto();
      expect(cartaoA, isNotNull);
      esquecerIdentidade();

      // B reabre o app, busca e lê.
      await cofre.abrir(List.of(pin));
      await adicionarContatoPorTexto(texto: cartaoA!);
      expect(await caixa.queimarLidas(), 0);
      relogio.reset();
      final recebidas = await correio.buscar();
      final msBuscar = relogio.elapsedMilliseconds;
      expect(recebidas.seladas, 1);
      expect(recebidas.naoAbriram, 0);
      expect(recebidas.caixaCheia, isFalse);
      final lista = await caixa.listar();
      expect(lista, hasLength(1));
      final lida = await caixa.ler(lista.first.gaveta);
      expect(lida.remetente, codigoA);
      expect(lida.texto, texto);
      expect(await caixa.queimarLidas(), 1);

      // O relay apagou a carta confirmada: nada mais a buscar.
      final deNovo = await correio.buscar();
      expect(deNovo.seladas + deNovo.naoAbriram, 0);
      // O estoque de B baixou uma chave (a que A retirou).
      expect(await correio.publicarChaves(manter: 5, minimo: 2), 4);

      // ignore: avoid_print
      print(
        'MEDIDA relay: publicar=${msPublicar}ms enviar=${msEnviar}ms '
        'buscar=${msBuscar}ms',
      );
      relay.fechar();
    },
  );
}
