// Teste no aparelho do cofre da identidade, com o núcleo em Rust e o Keystore de
// verdade. Usa pasta e alias próprios: não toca no cofre do app.
//
// Prova:
//   - criar, tirar da memória ("fechar") e abrir com o PIN devolve o mesmo código;
//   - PIN errado não abre;
//   - a chave HMAC ficou no hardware (TEE ou StrongBox), não em software;
//   - nenhuma das chaves secretas aparece em claro em arquivo nenhum do app.
//
// Imprime uma linha MEDIDA (tempo de criar e abrir, onde ficou a chave), que a
// ferramentas/suite.ps1 -Aparelho mostra no resultado. Nada de PIN, chave ou código.
//
// Rodar (com o celular no cabo):
//   flutter test integration_test

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:kryptos/cofre.dart';
import 'package:kryptos/cofre_do_aparelho.dart';
import 'package:kryptos/src/rust/api/nucleo.dart';
import 'package:kryptos/src/rust/frb_generated.dart';

const alias = 'kryptos-teste-cofre';
const pin = [1, 3, 5, 7, 9, 0];

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  late ChaveDoAparelho chave;
  late PastasDoApp pastas;
  late String pasta;
  late CofreDoAparelho cofre;

  Future<void> limpar() async {
    esquecerIdentidade();
    await cofreApagar(pasta: pasta);
    await chave.apagar(alias);
  }

  setUpAll(() async {
    await RustLib.init();
    chave = ChaveDoAparelho();
    pastas = await chave.pastas();
    pasta = '${pastas.semBackup}/teste-cofre';
    cofre = CofreDoAparelho(pasta: pasta, chave: chave, alias: alias);
  });

  setUp(limpar);
  tearDown(limpar);

  test('criar, fechar e abrir com o PIN devolve o mesmo código', () async {
    final codigo = gerarCodigoContato();
    final relogio = Stopwatch()..start();
    await cofre.criar(List.of(pin));
    final msCriar = relogio.elapsedMilliseconds;
    expect(cofre.existe(), isTrue);
    expect(await chave.existe(alias), isTrue);

    esquecerIdentidade();
    expect(codigoAtual(), isNull, reason: 'identidade continuou na memória');

    relogio.reset();
    final aberto = await cofre.abrir(List.of(pin));
    final msAbrir = relogio.elapsedMilliseconds;
    expect(aberto, codigo);
    expect(codigoAtual(), codigo);

    final nivel = await chave.nivel(alias);
    // ignore: avoid_print
    print('MEDIDA criar=${msCriar}ms abrir=${msAbrir}ms chave=$nivel');
  });

  test('PIN errado não abre e não põe identidade na memória', () async {
    gerarCodigoContato();
    await cofre.criar(List.of(pin));
    esquecerIdentidade();
    await expectLater(
      cofre.abrir([1, 3, 5, 7, 9, 1]),
      throwsA(isA<PinIncorreto>()),
    );
    expect(codigoAtual(), isNull);
  });

  test('a chave do aparelho ficou no hardware', () async {
    gerarCodigoContato();
    await cofre.criar(List.of(pin));
    expect(await chave.nivel(alias), anyOf('STRONGBOX', 'TEE', 'HARDWARE'));
  });

  test('sem a chave do Keystore o cofre não abre', () async {
    gerarCodigoContato();
    await cofre.criar(List.of(pin));
    esquecerIdentidade();
    await chave.apagar(alias);
    await expectLater(
      cofre.abrir(List.of(pin)),
      throwsA(
        isA<FalhaDoCofre>().having((e) => e.codigo, 'codigo', 'CHAVE_AUSENTE'),
      ),
    );
  });

  test('nenhuma chave secreta em claro em arquivo do app', () async {
    gerarCodigoContato();
    await cofre.criar(List.of(pin));
    esquecerIdentidade();
    await cofre.abrir(List.of(pin));
    // A varredura procura as chaves da identidade aberta, em 8 formas (bytes,
    // hex, base64, lista JSON), em todos os arquivos da pasta do app.
    final v = await procurarSegredos(pasta: pastas.dados);
    // ignore: avoid_print
    print(
      'MEDIDA varredura: ${v.arquivos} arquivos, ${v.ocorrencias} ocorrencias, ${v.ilegiveis} ilegiveis',
    );
    expect(v.arquivos, greaterThan(0));
    expect(v.ocorrencias, 0);
    expect(v.ilegiveis, 0);
  });
}
