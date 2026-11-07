// Tela provisória depois do PIN: o código da identidade aberta e os contatos desta
// execução. Dá lugar à Central (modo 01) quando ela for construída.
//
// A agenda fica só na memória: some quando o app fecha (cartao-de-contato.md).

import 'package:flutter/material.dart';

import '../contatos.dart';
import '../tema.dart';
import 'componentes.dart';
import 'novo_contato.dart';

class CofreAberto extends StatefulWidget {
  const CofreAberto({
    super.key,
    required this.versaoNucleo,
    required this.codigo,
    required this.agenda,
    required this.abrirCamera,
  });

  final String versaoNucleo;
  final String codigo;
  final Agenda agenda;
  final AbrirCamera abrirCamera;

  @override
  State<CofreAberto> createState() => _CofreAbertoState();
}

class _CofreAbertoState extends State<CofreAberto> {
  Future<void> _novoContato() async {
    await Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => NovoContato(
          versaoNucleo: widget.versaoNucleo,
          codigo: widget.codigo,
          agenda: widget.agenda,
          abrirCamera: widget.abrirCamera,
        ),
      ),
    );
    // A agenda pode ter mudado na outra tela.
    if (mounted) setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    final contatos = widget.agenda.contatos();
    return MoldeDeEntrada(
      titulo: 'COFRE ABERTO',
      versaoNucleo: widget.versaoNucleo,
      conteudo: [
        const Text('01 · SEU CÓDIGO', style: estiloRotulo),
        const SizedBox(height: 10),
        PainelCodigo(
          codigo: widget.codigo,
          legenda: 'A mesma identidade, aberta pelo PIN neste aparelho.',
        ),
        const SizedBox(height: 16),
        Text(
          '02 · CONTATOS (${contatos.length})',
          key: const Key('rotulo-contatos'),
          style: estiloRotulo,
        ),
        const SizedBox(height: 10),
        if (contatos.isEmpty)
          const Text(
            'Nenhum contato nesta execução. A agenda fica só na memória e some quando o app fecha.',
            style: estiloTextoCorrido,
          )
        else
          for (final c in contatos) _LinhaDeContato(c),
      ],
      acao: Botao(
        nome: 'NOVO CONTATO',
        rotulo: 'NOVO CONTATO',
        aoTocar: _novoContato,
      ),
    );
  }
}

class _LinhaDeContato extends StatelessWidget {
  const _LinhaDeContato(this.contato);

  final ContatoNaAgenda contato;

  @override
  Widget build(BuildContext context) {
    return Container(
      key: Key('contato-${contato.codigo}'),
      margin: const EdgeInsets.only(bottom: 6),
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
      decoration: BoxDecoration(
        color: Cores.painel,
        border: Border.all(color: Cores.linha),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.baseline,
        textBaseline: TextBaseline.alphabetic,
        children: [
          Text(
            contato.codigo,
            style: const TextStyle(
              fontSize: 16,
              fontWeight: FontWeight.w700,
              letterSpacing: 2,
              color: Cores.texto,
            ),
          ),
          const SizedBox(width: 12),
          Expanded(
            child: Text(
              contato.conferidoPorQr ? 'CHAVE CONFERIDA · QR' : 'NÃO CONFERIDA',
              textAlign: TextAlign.right,
              style: TextStyle(
                fontSize: 11,
                fontWeight: FontWeight.w700,
                letterSpacing: 1,
                color: contato.conferidoPorQr ? Cores.ambar : Cores.laranja,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
