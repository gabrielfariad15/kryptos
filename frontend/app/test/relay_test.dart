// Cliente do relay no computador, contra um servidor HTTP falso na própria máquina:
// regra de TLS, caminhos, corpo enviado, leitura das respostas e dos códigos de erro.

import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:kryptos/relay.dart';

void main() {
  late HttpServer servidor;
  late ClienteDoRelay cliente;
  final pedidos = <(String, Uint8List, String?)>[];
  var status = 200;
  var resposta = <int>[];

  setUp(() async {
    pedidos.clear();
    status = 200;
    resposta = <int>[];
    servidor = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    servidor.listen((r) async {
      final corpo = BytesBuilder(copy: false);
      await for (final p in r) {
        corpo.add(p);
      }
      pedidos.add((r.uri.path, corpo.takeBytes(), r.headers.contentType?.mimeType));
      r.response.statusCode = status;
      r.response.add(resposta);
      await r.response.close();
    });
    cliente = ClienteDoRelay(Uri.parse('http://127.0.0.1:${servidor.port}'));
  });

  tearDown(() async {
    cliente.fechar();
    await servidor.close(force: true);
  });

  test('só https, ou http para a própria máquina', () {
    for (final ok in [
      'https://relay.exemplo.com',
      'http://127.0.0.1:8080',
      'http://localhost:8080',
    ]) {
      expect(ClienteDoRelay.transporteSeguro(Uri.parse(ok)), isTrue, reason: ok);
    }
    for (final ruim in [
      'http://relay.exemplo.com',
      'http://10.0.0.2:8080',
      'http://192.168.0.10',
      'ftp://relay.exemplo.com',
      'https://',
    ]) {
      expect(ClienteDoRelay.transporteSeguro(Uri.parse(ruim)), isFalse, reason: ruim);
    }
    expect(
      () => ClienteDoRelay(Uri.parse('http://relay.exemplo.com')),
      throwsA(isA<FalhaDoRelay>().having((e) => e.codigo, 'codigo', 'RELAY_SEM_TLS')),
    );
  });

  test('publicar manda o corpo e lê o estoque', () async {
    resposta = utf8.encode('ESTOQUE 5');
    expect(await cliente.publicar([1, 2, 3]), 5);
    expect(pedidos, hasLength(1));
    expect(pedidos.first.$1, '/v1/publicar');
    expect(pedidos.first.$2, [1, 2, 3]);
    expect(pedidos.first.$3, 'application/octet-stream');
  });

  test('NUMERO_VELHO traz o último número aceito', () async {
    status = 409;
    resposta = utf8.encode('NUMERO_VELHO 42');
    await expectLater(
      cliente.publicar([1]),
      throwsA(
        isA<FalhaDoRelay>()
            .having((e) => e.codigo, 'codigo', 'NUMERO_VELHO')
            .having((e) => e.numero, 'numero', 42)
            .having((e) => e.status, 'status', 409),
      ),
    );
  });

  test('retirar, depositar e buscar usam as rotas certas', () async {
    resposta = List.filled(96, 7);
    expect(await cliente.retirar('ab' * 32), List.filled(96, 7));
    resposta = utf8.encode('GUARDADA');
    await cliente.depositar([9, 9]);
    resposta = [0, 0];
    expect(await cliente.buscar([4]), [0, 0]);
    expect(pedidos.map((p) => p.$1), [
      '/v1/retirar/${'ab' * 32}',
      '/v1/depositar',
      '/v1/buscar',
    ]);
  });

  test('erro do relay vira o código fixo; resposta estranha vira HTTP_<status>', () async {
    status = 404;
    resposta = utf8.encode('SEM_CHAVE');
    await expectLater(
      cliente.retirar('00' * 32),
      throwsA(isA<FalhaDoRelay>().having((e) => e.codigo, 'codigo', 'SEM_CHAVE')),
    );
    status = 200;
    resposta = utf8.encode('outra coisa');
    await expectLater(
      cliente.depositar([1]),
      throwsA(isA<FalhaDoRelay>().having((e) => e.codigo, 'codigo', 'HTTP_200')),
    );
  });

  test('resposta acima de 2 MiB é cortada', () async {
    resposta = Uint8List(bytesMaximoDaResposta + 1);
    await expectLater(
      cliente.buscar([1]),
      throwsA(isA<FalhaDoRelay>().having((e) => e.codigo, 'codigo', 'RESPOSTA_GRANDE')),
    );
  });

  test('relay fora do ar vira falha de rede', () async {
    final porta = servidor.port;
    await servidor.close(force: true);
    final semRelay = ClienteDoRelay(Uri.parse('http://127.0.0.1:$porta'));
    await expectLater(
      semRelay.buscar([1]),
      throwsA(isA<FalhaDoRelay>().having((e) => e.codigo, 'codigo', startsWith('REDE_'))),
    );
    semRelay.fechar();
  });
}
