// O que as telas sabem da caixa e do correio. Sem dependência da ponte: os testes de
// tela usam um mensageiro falso, e o de verdade (mensageiro_do_aparelho.dart) liga a
// caixa selada (núcleo + Keystore) ao relay.
//
// Falhas: o de verdade lança FalhaDoCofre (núcleo, Keystore) e FalhaDoRelay (relay,
// rede), só com o código fixo. Nunca texto, chave ou código de contato.

import 'dart:typed_data';

/// O que uma busca no relay trouxe.
class Recebidas {
  const Recebidas({
    required this.seladas,
    required this.naoAbriram,
    required this.caixaCheia,
  });

  /// Cartas seladas na caixa agora.
  final int seladas;

  /// Cartas que não abriram (conversa encerrada, estranho, adulterada).
  final int naoAbriram;

  /// A caixa encheu: ficaram cartas no relay.
  final bool caixaCheia;
}

/// Uma mensagem guardada na caixa, sem conteúdo.
class Selada {
  const Selada({
    required this.gaveta,
    required this.lida,
    required this.remetente,
  });

  final int gaveta;
  final bool lida;

  /// Código de quem mandou; vazio quando não se sabe (caixa antiga).
  final String remetente;
}

/// Uma mensagem aberta para a tela. [texto] é zerado por quem recebe, depois do uso.
class Aberta {
  const Aberta({required this.remetente, required this.texto});

  final String remetente;
  final Uint8List texto;
}

abstract class Mensageiro {
  /// O app foi montado com o endereço de um relay.
  bool get temRelay;

  /// Ao abrir o app: termina as queimas interrompidas e cria as chaves de gaveta que
  /// faltam no Keystore.
  Future<void> preparar();

  /// Repõe as chaves avulsas no relay. Sem relay, não faz nada.
  Future<void> publicarChaves();

  /// Busca as cartas no relay e sela na caixa. Sem relay, não traz nada.
  Future<Recebidas> buscar();

  /// Mensagens não queimadas, na ordem de chegada.
  Future<List<Selada>> listar();

  /// Cartas que não abriram nesta execução do app.
  Future<int> naoAbertas();

  /// Abre a mensagem. Na primeira vez marca como lida e o minuto até a queima começa.
  Future<Aberta> ler(int gaveta);

  /// Queima as lidas há um minuto ou mais. Devolve quantas queimou.
  Future<int> queimarVencidas();

  /// Queima na hora todas as lidas. Devolve quantas queimou.
  Future<int> queimarLidas();

  /// Cifra o texto para o contato e deixa a carta no relay.
  Future<void> enviar(String codigo, Uint8List texto);
}
