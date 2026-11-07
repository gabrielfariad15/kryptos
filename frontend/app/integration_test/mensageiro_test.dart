// Teste no aparelho do mensageiro que as telas usam (MensageiroDoAparelho): o mesmo
// caminho do relay_test, mas pelas peças que o app monta em main.dart, para provar a
// ligação entre elas (remetente na lista, leitura, queima).
//
// Num aparelho só, duas identidades, uma de cada vez na memória, como no relay_test.
// Gavetas com prefixo próprio: o teste não toca nas chaves das gavetas do app.
//
// Imprime uma linha MEDIDA com os tempos. Nada de texto, chave ou código.

import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:kryptos/caixa_do_aparelho.dart';
import 'package:kryptos/cofre_do_aparelho.dart';
import 'package:kryptos/correio.dart';
import 'package:kryptos/mensageiro_do_aparelho.dart';
import 'package:kryptos/relay.dart';
import 'package:kryptos/src/rust/api/nucleo.dart';
import 'package:kryptos/src/rust/frb_generated.dart';

const enderecoDoRelay = String.fromEnvironment('KRYPTOS_RELAY_TESTE');
const aliasDoCofre = 'kryptos-teste-mensageiro';
const prefixoDeTeste = 'kryptos-teste-mensageiro-gaveta-';
const pin = [4, 4, 5, 5, 6, 6];

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  late ChaveDoAparelho chave;
  late String pastaB;
  late CofreDoAparelho cofre;
  late CaixaDoAparelho caixa;
  late ClienteDoRelay relay;
  late MensageiroDoAparelho mensageiro;

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
    pastaB = '${pastas.semBackup}/teste-mensageiro';
    cofre = CofreDoAparelho(pasta: pastaB, chave: chave, alias: aliasDoCofre);
    caixa = CaixaDoAparelho(chave: chave, prefixo: prefixoDeTeste);
    if (enderecoDoRelay.isNotEmpty) {
      relay = ClienteDoRelay(Uri.parse(enderecoDoRelay));
      mensageiro = MensageiroDoAparelho(
        caixa: caixa,
        correio: Correio(relay: relay, caixa: caixa),
      );
    }
  });

  setUp(limpar);
  tearDown(limpar);

  test(
    'A manda pelo mensageiro, B busca, vê o remetente, lê e queima',
    skip: enderecoDoRelay.isEmpty
        ? 'sem relay: rode pela ferramentas/suite.ps1 -Aparelho'
        : false,
    timeout: const Timeout(Duration(minutes: 10)),
    () async {
      final relogio = Stopwatch()..start();

      // B abre o app pela primeira vez: cofre, gavetas e chaves avulsas no relay.
      gerarCodigoContato();
      await cofre.criar(List.of(pin));
      relogio.reset();
      await mensageiro.preparar();
      final msPreparar = relogio.elapsedMilliseconds;
      relogio.reset();
      await mensageiro.publicarChaves();
      final msPublicar = relogio.elapsedMilliseconds;
      final cartaoB = cartaoProprioTexto();
      expect(cartaoB, isNotNull);
      esquecerIdentidade();

      // A, sem cofre: lê o cartão de B e manda.
      final codigoA = gerarCodigoContato();
      final b = await adicionarContatoPorTexto(texto: cartaoB!);
      final texto = Uint8List.fromList(
        utf8.encode('PELO-MENSAGEIRO-${DateTime.now().microsecondsSinceEpoch}'),
      );
      await mensageiro.enviar(b.codigo, texto);
      final cartaoA = cartaoProprioTexto();
      expect(cartaoA, isNotNull);
      esquecerIdentidade();

      // B reabre: prepara, busca e vê de quem é antes de abrir.
      await cofre.abrir(List.of(pin));
      await adicionarContatoPorTexto(texto: cartaoA!);
      await mensageiro.preparar();
      relogio.reset();
      final recebidas = await mensageiro.buscar();
      final msBuscar = relogio.elapsedMilliseconds;
      expect(recebidas.seladas, 1);
      final lista = await mensageiro.listar();
      expect(lista, hasLength(1));
      expect(lista.single.remetente, codigoA);
      expect(lista.single.lida, isFalse);

      relogio.reset();
      final aberta = await mensageiro.ler(lista.single.gaveta);
      final msLer = relogio.elapsedMilliseconds;
      expect(aberta.remetente, codigoA);
      expect(aberta.texto, texto);
      expect((await mensageiro.listar()).single.lida, isTrue);
      // Lida pode ser aberta de novo até queimar (segurar outra vez).
      expect((await mensageiro.ler(lista.single.gaveta)).texto, texto);
      expect(await mensageiro.queimarVencidas(), 0, reason: 'antes do minuto');

      expect(await mensageiro.queimarLidas(), 1);
      expect(await mensageiro.listar(), isEmpty);
      expect(await mensageiro.naoAbertas(), 0);

      // ignore: avoid_print
      print(
        'MEDIDA mensageiro: preparar=${msPreparar}ms publicar=${msPublicar}ms '
        'buscar=${msBuscar}ms ler=${msLer}ms',
      );
      relay.fechar();
    },
  );
}
