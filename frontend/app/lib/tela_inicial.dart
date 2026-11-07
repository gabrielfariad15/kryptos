import 'package:flutter/material.dart';

import 'cofre.dart';
import 'contatos.dart';
import 'tema.dart';
import 'telas/cofre_aberto.dart';
import 'telas/desbloqueio.dart';
import 'telas/primeira_abertura.dart';

/// App sem dependência direta da ponte nem da câmera: recebe a versão do núcleo
/// pronta, a função que gera o código, o cofre, a agenda e a câmera, para os testes
/// de tela rodarem no computador sem a biblioteca em Rust e sem o plugin.
///
/// Sem cofre gravado: primeira abertura. Com cofre: desbloqueio. Depois de um dos
/// dois: o cofre aberto, com o código.
class KryptosApp extends StatelessWidget {
  const KryptosApp({
    super.key,
    required this.versaoNucleo,
    required this.gerarCodigo,
    required this.cofre,
    required this.agenda,
    required this.abrirCamera,
  });

  final String versaoNucleo;
  final String Function() gerarCodigo;
  final Cofre cofre;
  final Agenda agenda;
  final AbrirCamera abrirCamera;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Kryptos',
      debugShowCheckedModeBanner: false,
      theme: temaKryptos(),
      home: _Entrada(
        versaoNucleo: versaoNucleo,
        gerarCodigo: gerarCodigo,
        cofre: cofre,
        agenda: agenda,
        abrirCamera: abrirCamera,
      ),
    );
  }
}

class _Entrada extends StatefulWidget {
  const _Entrada({
    required this.versaoNucleo,
    required this.gerarCodigo,
    required this.cofre,
    required this.agenda,
    required this.abrirCamera,
  });

  final String versaoNucleo;
  final String Function() gerarCodigo;
  final Cofre cofre;
  final Agenda agenda;
  final AbrirCamera abrirCamera;

  @override
  State<_Entrada> createState() => _EntradaState();
}

class _EntradaState extends State<_Entrada> {
  late final bool _temCofre = widget.cofre.existe();
  String? _codigo;

  void _aberto(String codigo) => setState(() => _codigo = codigo);

  @override
  Widget build(BuildContext context) {
    final codigo = _codigo;
    if (codigo != null) {
      return CofreAberto(
        versaoNucleo: widget.versaoNucleo,
        codigo: codigo,
        agenda: widget.agenda,
        abrirCamera: widget.abrirCamera,
      );
    }
    if (_temCofre) {
      return Desbloqueio(
        versaoNucleo: widget.versaoNucleo,
        cofre: widget.cofre,
        aoAbrir: _aberto,
      );
    }
    return PrimeiraAbertura(
      versaoNucleo: widget.versaoNucleo,
      gerarCodigo: widget.gerarCodigo,
      cofre: widget.cofre,
      aoConcluir: _aberto,
    );
  }
}
