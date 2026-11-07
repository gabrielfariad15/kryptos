// Cliente HTTP do relay: só leva e traz bytes. Quem monta e assina o que vai, e lê o
// que volta, é o núcleo em Rust (correio.dart). Rotas e respostas:
// documentacao/arquitetura/estoque-de-chaves.md e malote.md.
//
// Sem pacote de fora: o HttpClient do dart:io. Ele usa sockets do próprio Dart, que não
// passam pela política de rede do Android, então a regra de TLS mora aqui: só https,
// com exceção de 127.0.0.1 e localhost (o relay de teste no computador, pelo
// `adb reverse`).
//
// Nada de corpo, dono ou carta em log (regra 8.2).

import 'dart:io';
import 'dart:typed_data';

/// Falha do relay ou da rede, com o código fixo da resposta (NUMERO_VELHO,
/// MALOTE_CHEIO, SEM_CHAVE...) ou um código local (RELAY_SEM_TLS, REDE, RESPOSTA_GRANDE).
class FalhaDoRelay implements Exception {
  const FalhaDoRelay(this.codigo, {this.status = 0, this.numero});

  final String codigo;

  /// Status HTTP (0 quando nem chegou resposta).
  final int status;

  /// Em NUMERO_VELHO: o último número que o relay aceitou.
  final int? numero;

  @override
  String toString() => 'FalhaDoRelay($codigo, $status)';
}

/// Maior resposta aceita: a entrega do malote tem até 1 MiB, com folga.
const bytesMaximoDaResposta = 2 << 20;

class ClienteDoRelay {
  ClienteDoRelay(this.base, {HttpClient? http})
    : _http = http ?? HttpClient() {
    if (!transporteSeguro(base)) throw const FalhaDoRelay('RELAY_SEM_TLS');
    _http.connectionTimeout = const Duration(seconds: 10);
  }

  final Uri base;
  final HttpClient _http;

  /// https sempre; http só para a própria máquina (relay de teste).
  static bool transporteSeguro(Uri u) {
    if (u.scheme == 'https' && u.host.isNotEmpty) return true;
    return u.scheme == 'http' && (u.host == '127.0.0.1' || u.host == 'localhost');
  }

  /// Publica (ou só consulta, sem chave) e devolve quantas chaves o dono tem no relay.
  Future<int> publicar(List<int> publicacao) async {
    final r = await _post('/v1/publicar', publicacao);
    final m = RegExp(r'^ESTOQUE (\d+)$').firstMatch(_texto(r.corpo));
    if (r.status != 200 || m == null) throw _falha(r);
    return int.parse(m.group(1)!);
  }

  /// A chave avulsa do dono (96 bytes), uma vez só. Sem chave: FalhaDoRelay(SEM_CHAVE).
  Future<Uint8List> retirar(String dono) async {
    final r = await _post('/v1/retirar/$dono', const []);
    if (r.status != 200) throw _falha(r);
    return r.corpo;
  }

  Future<void> depositar(List<int> carta) async {
    final r = await _post('/v1/depositar', carta);
    if (r.status != 200 || _texto(r.corpo) != 'GUARDADA') throw _falha(r);
  }

  /// A entrega do malote (bytes que o núcleo lê).
  Future<Uint8List> buscar(List<int> busca) async {
    final r = await _post('/v1/buscar', busca);
    if (r.status != 200) throw _falha(r);
    return r.corpo;
  }

  void fechar() => _http.close(force: true);

  static String _texto(Uint8List b) => String.fromCharCodes(b).trim();

  static FalhaDoRelay _falha(_Resposta r) {
    final partes = _texto(r.corpo).split(' ');
    final codigo = partes.isNotEmpty && RegExp(r'^[A-Z_]+$').hasMatch(partes.first)
        ? partes.first
        : 'HTTP_${r.status}';
    final numero = partes.length > 1 ? int.tryParse(partes[1]) : null;
    return FalhaDoRelay(codigo, status: r.status, numero: numero);
  }

  Future<_Resposta> _post(String caminho, List<int> corpo) async {
    try {
      final pedido = await _http
          .postUrl(base.resolve(caminho))
          .timeout(const Duration(seconds: 15));
      pedido.headers.contentType = ContentType.binary;
      pedido.contentLength = corpo.length;
      pedido.add(corpo);
      final resposta = await pedido.close().timeout(const Duration(seconds: 30));
      final dados = BytesBuilder(copy: false);
      await for (final pedaco in resposta.timeout(const Duration(seconds: 30))) {
        dados.add(pedaco);
        if (dados.length > bytesMaximoDaResposta) {
          throw const FalhaDoRelay('RESPOSTA_GRANDE');
        }
      }
      return _Resposta(resposta.statusCode, dados.takeBytes());
    } on FalhaDoRelay {
      rethrow;
    } on Exception catch (e) {
      // Só o tipo: a mensagem pode trazer o endereço do relay.
      throw FalhaDoRelay('REDE_${e.runtimeType}');
    }
  }
}

class _Resposta {
  const _Resposta(this.status, this.corpo);

  final int status;
  final Uint8List corpo;
}
