// Tela provisória depois do PIN: mostra o código da identidade aberta. Dá lugar à
// Central (modo 01) quando ela for construída.

import 'package:flutter/material.dart';

import '../tema.dart';
import 'componentes.dart';

class CofreAberto extends StatelessWidget {
  const CofreAberto({
    super.key,
    required this.versaoNucleo,
    required this.codigo,
  });

  final String versaoNucleo;
  final String codigo;

  @override
  Widget build(BuildContext context) {
    return MoldeDeEntrada(
      titulo: 'COFRE ABERTO',
      versaoNucleo: versaoNucleo,
      conteudo: [
        const Text('01 · SEU CÓDIGO', style: estiloRotulo),
        const SizedBox(height: 10),
        PainelCodigo(
          codigo: codigo,
          legenda: 'A mesma identidade, aberta pelo PIN neste aparelho.',
        ),
      ],
    );
  }
}
