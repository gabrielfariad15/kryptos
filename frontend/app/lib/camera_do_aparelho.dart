// Câmera de verdade: plugin `camera` (no Android, o camera_android_camerax sobre o
// CameraX). Só a traseira, só quadros para o leitor e a prévia na tela: sem áudio,
// sem foto, sem vídeo, nada gravado.
//
// Formato do quadro: YUV 420. O leitor usa só o plano Y (brilho) com o bytesPerRow
// dele, porque o CameraX completa as linhas além da largura. Não usamos o NV21: com
// ele o plugin continua dizendo yuv420, e o formato declarado deixaria de ser prova.
//
// Resolução média (480p): estimo que baste para o QR do cartão a um palmo, e cada
// quadro atravessa Java → Dart → Rust. É a primeira variável a mexer se a medição
// no aparelho não der 9 em 10 (cartao-de-contato.md).

import 'package:camera/camera.dart';
import 'package:flutter/widgets.dart';

import 'contatos.dart';

class CameraDoAparelho implements Camera {
  CameraController? _controle;
  bool _fechada = false;

  @override
  Future<void> abrir() async {
    try {
      final cameras = await availableCameras();
      final traseiras = cameras.where(
        (c) => c.lensDirection == CameraLensDirection.back,
      );
      if (traseiras.isEmpty) throw const FalhaDaCamera('SEM_CAMERA');
      final controle = CameraController(
        traseiras.first,
        ResolutionPreset.medium,
        enableAudio: false,
        imageFormatGroup: ImageFormatGroup.yuv420,
      );
      try {
        await controle.initialize();
      } catch (_) {
        await controle.dispose();
        rethrow;
      }
      // Fechada enquanto ligava (a pessoa saiu da tela): solta aqui mesmo.
      if (_fechada) {
        await controle.dispose();
        return;
      }
      _controle = controle;
    } on CameraException catch (e) {
      throw FalhaDaCamera(e.code);
    }
  }

  @override
  Widget previa() {
    final controle = _controle;
    if (controle == null || !controle.value.isInitialized) {
      return const SizedBox.shrink();
    }
    // A prévia vem deitada (largura/altura do sensor). Em pé, a altura é a largura
    // vezes a proporção. O FittedBox corta o que sobra para caber no quadrado.
    return FittedBox(
      fit: BoxFit.cover,
      clipBehavior: Clip.hardEdge,
      child: SizedBox(
        width: 100,
        height: 100 * controle.value.aspectRatio,
        child: CameraPreview(controle),
      ),
    );
  }

  @override
  Future<void> receberQuadros(void Function(Quadro quadro) aoQuadro) async {
    final controle = _controle;
    if (controle == null || _fechada) return;
    try {
      await controle.startImageStream((imagem) {
        if (imagem.planes.isEmpty) return;
        final y = imagem.planes.first;
        aoQuadro(
          Quadro(
            brilho: y.bytes,
            largura: imagem.width,
            altura: imagem.height,
            passo: y.bytesPerRow,
          ),
        );
      });
    } on CameraException catch (e) {
      throw FalhaDaCamera(e.code);
    }
  }

  @override
  Future<void> fechar() async {
    if (_fechada) return;
    _fechada = true;
    final controle = _controle;
    _controle = null;
    if (controle == null) return;
    try {
      if (controle.value.isStreamingImages) await controle.stopImageStream();
    } on CameraException catch (e) {
      debugPrint('Kryptos: falha ao parar os quadros (${e.code})');
    }
    await controle.dispose();
  }
}
