// Modo 01 · Central: os canais (contatos da agenda e quem mandou mensagem), as novas
// por canal e o estado do relay e da caixa. Visual aprovado em 07/11/2026
// (U1Central). Tocar num canal abre a conversa (modo 02).
//
// A agenda fica gravada cifrada no cofre (agenda-gravada.md); as mensagens novas ficam
// seladas na caixa até serem lidas (caixa-selada.md).

import 'package:flutter/material.dart';

import '../contatos.dart';
import '../plantao.dart';
import '../tema.dart';
import 'componentes.dart';
import 'conversa.dart';
import 'novo_contato.dart';

/// Um canal da Central: um contato da agenda ou um remetente fora dela.
class Canal {
  const Canal({
    required this.codigo,
    required this.naAgenda,
    required this.conferido,
  });

  /// Código de contato; vazio para as mensagens sem remetente (caixa antiga).
  final String codigo;
  final bool naAgenda;
  final bool conferido;

  String get nome => codigo.isEmpty ? 'SEM REMETENTE' : codigo;

  String get situacao {
    if (codigo.isEmpty) return 'CAIXA ANTIGA · SÓ LEITURA';
    if (!naAgenda) return 'FORA DA AGENDA · SÓ LEITURA';
    return conferido ? 'CONVERSA · CHAVE CONFERIDA' : 'CHAVE NÃO CONFERIDA';
  }

  bool get alerta => !naAgenda || !conferido;
}

/// Contatos da agenda na ordem em que entraram, depois os remetentes que não estão
/// nela, na ordem de chegada.
List<Canal> canais(List<ContatoNaAgenda> agenda, Plantao plantao) {
  final lista = [
    for (final c in agenda)
      Canal(codigo: c.codigo, naAgenda: true, conferido: c.conferidoPorQr),
  ];
  final vistos = {for (final c in agenda) c.codigo};
  for (final s in plantao.seladas) {
    if (vistos.add(s.remetente)) {
      lista.add(Canal(codigo: s.remetente, naAgenda: false, conferido: false));
    }
  }
  return lista;
}

class Central extends StatefulWidget {
  const Central({
    super.key,
    required this.versaoNucleo,
    required this.codigo,
    required this.agenda,
    required this.abrirCamera,
    required this.plantao,
  });

  final String versaoNucleo;
  final String codigo;
  final Agenda agenda;
  final AbrirCamera abrirCamera;
  final Plantao plantao;

  @override
  State<Central> createState() => _CentralState();
}

class _CentralState extends State<Central> {
  String? _ultimoCanal;

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

  Future<void> _abrirConversa(Canal canal) async {
    setState(() => _ultimoCanal = canal.codigo);
    await Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => Conversa(
          plantao: widget.plantao,
          codigo: canal.codigo,
          podeEnviar: canal.naAgenda && widget.plantao.temRelay,
          motivoSemEnvio: !canal.naAgenda
              ? 'Este remetente não está na agenda: só dá para ler.'
              : 'App montado sem relay: não há para onde mandar.',
        ),
      ),
    );
    if (mounted) setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: widget.plantao,
      builder: (context, _) {
        final p = widget.plantao;
        final lista = canais(widget.agenda.contatos(), p);
        final ultimo = lista.where((c) => c.codigo == _ultimoCanal).firstOrNull;
        return AnnotatedRegion(
          value: estiloBarras,
          child: Scaffold(
            body: SafeArea(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  _Cabecalho(codigo: widget.codigo, plantao: p),
                  Expanded(
                    child: SingleChildScrollView(
                      padding: const EdgeInsets.fromLTRB(18, 16, 18, 12),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          Text(
                            'CANAIS (${lista.length})',
                            key: const Key('rotulo-canais'),
                            style: estiloRotulo,
                          ),
                          const SizedBox(height: 10),
                          if (lista.isEmpty)
                            const Text(
                              'Nenhum contato. Toque em NOVO CONTATO e leia o QR '
                              'do contato, frente a frente. A agenda fica gravada '
                              'cifrada neste aparelho.',
                              style: estiloTextoCorrido,
                            )
                          else
                            for (final c in lista)
                              _LinhaDeCanal(
                                canal: c,
                                novas: p.novasDe(c.codigo),
                                aoTocar: () => _abrirConversa(c),
                              ),
                          ..._avisos(p),
                          const SizedBox(height: 14),
                          Botao(
                            nome: 'NOVO CONTATO',
                            rotulo: 'NOVO CONTATO',
                            aoTocar: _novoContato,
                          ),
                          const SizedBox(height: 14),
                          Text(
                            'NÚCLEO v${widget.versaoNucleo} · BUILD DE TESTE',
                            key: const Key('versao-nucleo'),
                            style: estiloRotulo,
                          ),
                        ],
                      ),
                    ),
                  ),
                  BarraDeModos(
                    aoCentral: null,
                    aoConversa: ultimo == null
                        ? null
                        : () => _abrirConversa(ultimo),
                  ),
                ],
              ),
            ),
          ),
        );
      },
    );
  }

  List<Widget> _avisos(Plantao p) {
    final avisos = <(String, String)>[
      if (p.naoAbertas > 0)
        (
          'aviso-nao-abertas',
          p.naoAbertas == 1
              ? '1 mensagem não pôde ser aberta (conversa encerrada, remetente '
                    'fora da agenda ou carta adulterada).'
              : '${p.naoAbertas} mensagens não puderam ser abertas (conversa '
                    'encerrada, remetente fora da agenda ou carta adulterada).',
        ),
      if (p.caixaCheia)
        (
          'aviso-caixa-cheia',
          'Caixa cheia: há cartas esperando no relay. Leia as mensagens para '
              'liberar gavetas.',
        ),
      if (p.falhaDoRelay case final f?)
        (
          'aviso-relay',
          'Relay fora do alcance ($f). Tentando de novo a cada 10 s.',
        ),
      if (p.falhaDaCaixa case final f?) ('aviso-caixa', 'Falha na caixa ($f).'),
    ];
    return [
      for (final (chave, texto) in avisos) ...[
        const SizedBox(height: 10),
        Text(texto, key: Key(chave), style: estiloAviso),
      ],
    ];
  }
}

const _estiloLinha = TextStyle(
  fontSize: 11,
  letterSpacing: 1,
  color: Cores.secundario,
);

class _Cabecalho extends StatelessWidget {
  const _Cabecalho({required this.codigo, required this.plantao});

  final String codigo;
  final Plantao plantao;

  @override
  Widget build(BuildContext context) {
    final (relay, corRelay) = !plantao.temRelay
        ? ('SEM RELAY', Cores.secundario)
        : plantao.falhaDoRelay != null
        ? ('RELAY FORA', Cores.laranja)
        : ('RELAY LIGADO', Cores.secundario);
    return Container(
      padding: const EdgeInsets.fromLTRB(18, 16, 18, 14),
      decoration: const BoxDecoration(
        border: Border(bottom: BorderSide(color: Cores.ambar, width: 2)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const Row(
            crossAxisAlignment: CrossAxisAlignment.baseline,
            textBaseline: TextBaseline.alphabetic,
            children: [
              Text(
                'KRYPTOS',
                style: TextStyle(
                  fontSize: 22,
                  fontWeight: FontWeight.w700,
                  letterSpacing: 6,
                  color: Cores.ambar,
                ),
              ),
              SizedBox(width: 12),
              Expanded(
                child: Text(
                  'MODO 01 · CENTRAL',
                  textAlign: TextAlign.right,
                  style: estiloRotulo,
                ),
              ),
            ],
          ),
          const SizedBox(height: 8),
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: Wrap(
                  children: [
                    const Text('OPERADOR ', style: _estiloLinha),
                    Text(codigo, key: const Key('codigo'), style: _estiloLinha),
                  ],
                ),
              ),
              const SizedBox(width: 12),
              Text(
                relay,
                key: const Key('estado-relay'),
                style: TextStyle(
                  fontSize: 11,
                  letterSpacing: 1,
                  color: corRelay,
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _LinhaDeCanal extends StatelessWidget {
  const _LinhaDeCanal({
    required this.canal,
    required this.novas,
    required this.aoTocar,
  });

  final Canal canal;
  final int novas;
  final VoidCallback aoTocar;

  @override
  Widget build(BuildContext context) {
    final rotuloNovas = novas == 1 ? '1 NOVA' : '$novas NOVAS';
    return Padding(
      padding: const EdgeInsets.only(bottom: 10),
      child: Semantics(
        button: true,
        label:
            '${canal.nome}, ${canal.situacao}${novas > 0 ? ', $rotuloNovas' : ''}',
        excludeSemantics: true,
        child: InkWell(
          key: Key('canal-${canal.codigo}'),
          onTap: aoTocar,
          child: Container(
            constraints: const BoxConstraints(minHeight: 54),
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            decoration: BoxDecoration(
              color: canal.alerta ? null : Cores.painel,
              border: Border.all(
                color: canal.alerta ? Cores.laranja : Cores.linha,
              ),
            ),
            child: Row(
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        canal.nome,
                        style: const TextStyle(
                          fontSize: 14,
                          fontWeight: FontWeight.w600,
                          color: Cores.texto,
                        ),
                      ),
                      const SizedBox(height: 2),
                      Text(
                        canal.situacao,
                        style: TextStyle(
                          fontSize: 11,
                          color: canal.alerta
                              ? Cores.laranja
                              : Cores.secundario,
                        ),
                      ),
                    ],
                  ),
                ),
                if (novas > 0) ...[
                  const SizedBox(width: 10),
                  Container(
                    key: Key('novas-${canal.codigo}'),
                    padding: const EdgeInsets.symmetric(
                      horizontal: 8,
                      vertical: 3,
                    ),
                    color: Cores.ambar,
                    child: Text(
                      rotuloNovas,
                      style: const TextStyle(
                        fontSize: 11,
                        fontWeight: FontWeight.w700,
                        color: Cores.fundo,
                      ),
                    ),
                  ),
                ],
                // Mostra que a linha abre a conversa (no teste de 07/11/2026 o canal
                // não parecia tocável).
                const SizedBox(width: 6),
                Icon(
                  Icons.chevron_right,
                  key: Key('abrir-${canal.codigo}'),
                  size: 22,
                  color: canal.alerta ? Cores.laranja : Cores.ambar,
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Barra dos modos. Grupo e Direto ficam para a próxima fase: aparecem apagados e
/// não respondem ao toque. Conversa abre o último canal aberto, quando há um.
class BarraDeModos extends StatelessWidget {
  const BarraDeModos({
    super.key,
    required this.aoCentral,
    required this.aoConversa,
  });

  /// Nada quando a Central já é a tela atual.
  final VoidCallback? aoCentral;
  final VoidCallback? aoConversa;

  @override
  Widget build(BuildContext context) {
    Widget modo(
      String rotulo, {
      required bool atual,
      VoidCallback? aoTocar,
      String? nota,
    }) {
      final ligado = atual || aoTocar != null;
      return Expanded(
        child: Semantics(
          button: true,
          selected: atual,
          enabled: ligado,
          label: nota == null ? rotulo : '$rotulo, $nota',
          excludeSemantics: true,
          child: InkWell(
            key: Key('modo-$rotulo'),
            onTap: atual ? null : aoTocar,
            child: Container(
              // Altura mínima do visual; com fonte grande do sistema, cresce.
              constraints: const BoxConstraints(minHeight: 60),
              padding: const EdgeInsets.symmetric(vertical: 6),
              alignment: Alignment.center,
              decoration: BoxDecoration(
                border: Border(
                  top: BorderSide(
                    color: atual ? Cores.ambar : Colors.transparent,
                    width: 2,
                  ),
                ),
              ),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(
                    rotulo,
                    textAlign: TextAlign.center,
                    style: TextStyle(
                      fontSize: 11,
                      letterSpacing: 1,
                      fontWeight: atual ? FontWeight.w700 : FontWeight.w400,
                      color: atual
                          ? Cores.ambar
                          : ligado
                          ? Cores.secundario
                          : Cores.linhaTracejada,
                    ),
                  ),
                  if (nota != null)
                    Text(
                      nota,
                      textAlign: TextAlign.center,
                      style: const TextStyle(
                        fontSize: 8,
                        letterSpacing: 1,
                        color: Cores.linhaTracejada,
                      ),
                    ),
                ],
              ),
            ),
          ),
        ),
      );
    }

    return Container(
      decoration: const BoxDecoration(
        border: Border(top: BorderSide(color: Cores.linha)),
      ),
      child: Row(
        children: [
          modo('CENTRAL', atual: aoCentral == null, aoTocar: aoCentral),
          modo('CONVERSA', atual: false, aoTocar: aoConversa),
          modo('GRUPO', atual: false, nota: 'PRÓXIMA FASE'),
          modo('DIRETO', atual: false, nota: 'PRÓXIMA FASE'),
        ],
      ),
    );
  }
}
