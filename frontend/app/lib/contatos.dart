// O que as telas sabem da agenda de contatos e da câmera. Sem dependência da ponte
// nem do plugin da câmera: os testes de tela usam versões falsas, e as de verdade
// ficam em agenda_do_aparelho.dart e camera_do_aparelho.dart.
//
// Decisões: documentacao/arquitetura/cartao-de-contato.md.

import 'dart:typed_data';

import 'package:flutter/widgets.dart';

/// Grade do QR deste aparelho: [lado] × [lado] módulos, linha por linha, 1 = escuro.
/// A tela desenha [margem] módulos claros em volta (o padrão do QR pede 4).
class GradeQr {
  const GradeQr({
    required this.lado,
    required this.margem,
    required this.modulos,
  });

  final int lado;
  final int margem;
  final Uint8List modulos;

  bool escuro(int x, int y) =>
      x >= 0 && y >= 0 && x < lado && y < lado && modulos[y * lado + x] == 1;
}

/// Plano de brilho (Y) de um quadro da câmera: 1 byte por pixel. [passo] é o número
/// de bytes de uma linha no buffer, que a câmera pode completar além da largura.
class Quadro {
  const Quadro({
    required this.brilho,
    required this.largura,
    required this.altura,
    required this.passo,
  });

  final Uint8List brilho;
  final int largura;
  final int altura;
  final int passo;
}

/// Contato achado num quadro.
class ContatoLido {
  const ContatoLido({required this.codigo, required this.novo});

  final String codigo;

  /// `false`: já estava na agenda com as mesmas chaves.
  final bool novo;
}

class ContatoNaAgenda {
  const ContatoNaAgenda({required this.codigo, required this.conferidoPorQr});

  final String codigo;
  final bool conferidoPorQr;
}

/// Um QR foi achado e recusado, ou o quadro não pôde ser lido. Traz só o código fixo
/// do núcleo (QR_PROPRIO, QR_NAO_CONFERE...), nunca o texto lido.
class QrRecusado implements Exception {
  const QrRecusado(this.codigo);

  final String codigo;

  @override
  String toString() => 'QrRecusado($codigo)';
}

/// Agenda desta execução do app (só em memória, no núcleo).
abstract class Agenda {
  /// Grade do QR deste aparelho, ou nada sem identidade aberta.
  GradeQr? gradePropria();

  /// Procura um QR no quadro. Nada: o quadro não tem QR legível.
  /// Lança [QrRecusado] quando acha um QR que não entra na agenda.
  Future<ContatoLido?> lerQuadro(Quadro quadro);

  /// Contatos na ordem em que entraram.
  List<ContatoNaAgenda> contatos();
}

/// A câmera não abriu. [codigo] é o do plugin (CameraAccessDenied...) ou um nosso
/// (SEM_CAMERA).
class FalhaDaCamera implements Exception {
  const FalhaDaCamera(this.codigo);

  final String codigo;

  @override
  String toString() => 'FalhaDaCamera($codigo)';
}

/// Câmera traseira, só para ler o QR. Uma por leitura: abrir, receber, fechar.
abstract class Camera {
  /// Pede a permissão (na primeira vez) e liga a câmera. Lança [FalhaDaCamera].
  Future<void> abrir();

  /// A imagem ao vivo. Só depois de [abrir].
  Widget previa();

  /// Começa a mandar os quadros para [aoQuadro], no ritmo da câmera.
  Future<void> receberQuadros(void Function(Quadro quadro) aoQuadro);

  /// Para os quadros e solta a câmera. Pode ser chamada mais de uma vez.
  Future<void> fechar();
}

typedef AbrirCamera = Camera Function();

/// Texto do aviso para cada recusa. Laranja na tela: é alerta.
String mensagemDaRecusa(String codigo) => switch (codigo) {
  'QR_NAO_E_KRYPTOS' => 'Este QR não é de um contato do Kryptos.',
  'QR_VERSAO_DESCONHECIDA' =>
    'QR do Kryptos de uma versão que este app não conhece. Atualize o app.',
  'QR_TAMANHO' || 'QR_FORMATO' =>
    'QR do Kryptos fora do formato. O contato não foi adicionado.',
  'QR_NAO_CONFERE' => 'O QR não confere com o código: danificado ou adulterado. O contato não foi adicionado.',
  'QR_CHAVE_INVALIDA' =>
    'A chave deste QR é inválida. O contato não foi adicionado.',
  'QR_PROPRIO' => 'Este é o seu próprio QR. Aponte para o QR do contato.',
  'QR_CONFLITO' => 'Atenção: este QR imita um contato da agenda ou este aparelho. Não foi adicionado.',
  'SEM_IDENTIDADE' => 'Abra o cofre antes de adicionar contatos.',
  'QUADRO_INVALIDO' || 'QR_LEITOR' =>
    'A câmera mandou um quadro que não deu para ler. Continue apontando.',
  _ => 'Não foi possível ler o QR ($codigo).',
};

/// Texto do aviso quando a câmera não abre.
String mensagemDaCamera(String codigo) => switch (codigo) {
  'CameraAccessDenied' => 'Sem permissão para a câmera. Libere em Configurações, Apps, Kryptos, Permissões.',
  'CameraPermissionsRequestOngoing' =>
    'O pedido de permissão ainda está aberto. Responda e toque de novo.',
  'SEM_CAMERA' => 'Este aparelho não tem câmera traseira.',
  _ => 'Não foi possível abrir a câmera ($codigo).',
};
