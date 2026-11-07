// Modo 02 · Conversa: as mensagens de um canal, fechadas em grupos de letras até o
// usuário segurar. Visual aprovado em 07/11/2026 (U2Conversa).
//
// Regras (aprovadas em 07/11/2026):
//   - Segurar abre: a primeira vez decifra e o minuto até a queima começa; o texto só
//     aparece enquanto o dedo está na mensagem e some ao soltar.
//   - Fechada mostra sempre 8 grupos, para o tamanho do texto não aparecer.
//   - O texto some se o app sai da frente.
//   - Enviadas: só na memória, queimam 60 s depois de enviadas.
//   - Sem confirmação de leitura nem aviso de que se está escrevendo.
//
// Nada de texto ou código de contato em log (regra 8.2).

import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';

import 'package:flutter/material.dart';

import '../mensageiro.dart';
import '../plantao.dart';
import '../tema.dart';
import 'componentes.dart';

/// Grupos de letras de uma mensagem fechada: sempre o mesmo número, qualquer que seja
/// o tamanho do texto.
const gruposFechados = 8;

class Conversa extends StatefulWidget {
  const Conversa({
    super.key,
    required this.plantao,
    required this.codigo,
    required this.podeEnviar,
    required this.motivoSemEnvio,
  });

  final Plantao plantao;

  /// Código do canal; vazio para as mensagens sem remetente.
  final String codigo;
  final bool podeEnviar;

  /// Aparece no lugar do campo de texto quando [podeEnviar] é falso.
  final String motivoSemEnvio;

  @override
  State<Conversa> createState() => _ConversaState();
}

/// Uma linha da conversa: recebida (gaveta da caixa) ou enviada (memória).
class _Linha {
  const _Linha.recebida(this.selada, this.em) : enviada = null;
  _Linha.enviada(Enviada this.enviada) : selada = null, em = enviada.em;

  final Selada? selada;
  final Enviada? enviada;
  final DateTime em;

  String get id =>
      selada != null ? 'rx-${selada!.gaveta}' : 'tx-${enviada!.numero}';
}

class _ConversaState extends State<Conversa> {
  final _campo = TextEditingController();
  String? _segurando;
  String? _texto;
  Uint8List? _bytes;
  bool _enviando = false;
  String? _falhaDoEnvio;

  @override
  void initState() {
    super.initState();
    widget.plantao.addListener(_aoMudar);
    _campo.addListener(_aoDigitar);
  }

  @override
  void dispose() {
    widget.plantao.removeListener(_aoMudar);
    _campo.removeListener(_aoDigitar);
    _fecharTexto();
    _campo.dispose();
    super.dispose();
  }

  void _aoDigitar() => setState(() {});

  void _aoMudar() {
    // Saiu da frente, ou a mensagem aberta queimou: o texto some.
    final id = _segurando;
    if (id != null && (!widget.plantao.naFrente || !_existe(id))) {
      setState(_fecharTexto);
    }
  }

  bool _existe(String id) => _linhas().any((l) => l.id == id);

  List<_Linha> _linhas() {
    final p = widget.plantao;
    final linhas = <_Linha>[
      for (final s in p.seladasDe(widget.codigo))
        _Linha.recebida(s, p.chegouEm(s.gaveta) ?? p.agora()),
      for (final e in p.enviadasPara(widget.codigo)) _Linha.enviada(e),
    ];
    // Estável: a ordem da caixa vale entre recebidas da mesma hora.
    final ordem = {for (var i = 0; i < linhas.length; i++) linhas[i]: i};
    linhas.sort((a, b) {
      final c = a.em.compareTo(b.em);
      return c != 0 ? c : ordem[a]!.compareTo(ordem[b]!);
    });
    return linhas;
  }

  Future<void> _segurar(_Linha linha) async {
    final id = linha.id;
    setState(() {
      _fecharTexto();
      _segurando = id;
    });
    final enviada = linha.enviada;
    if (enviada != null) {
      // Texto da memória: mostra uma cópia do que foi mandado; o original queima
      // com a enviada.
      if (widget.plantao.naFrente) {
        setState(
          () => _texto = utf8.decode(enviada.texto, allowMalformed: true),
        );
      }
      return;
    }
    final aberta = await widget.plantao.abrir(linha.selada!.gaveta);
    if (aberta == null) return;
    if (!mounted || _segurando != id || !widget.plantao.naFrente) {
      // Soltou antes de o núcleo responder: nada aparece.
      aberta.texto.fillRange(0, aberta.texto.length, 0);
      return;
    }
    setState(() {
      _bytes = aberta.texto;
      _texto = utf8.decode(aberta.texto, allowMalformed: true);
    });
  }

  void _soltar() {
    if (_segurando == null) return;
    setState(_fecharTexto);
  }

  void _fecharTexto() {
    final b = _bytes;
    b?.fillRange(0, b.length, 0);
    _bytes = null;
    _texto = null;
    _segurando = null;
  }

  Future<void> _cifrar() async {
    final texto = _campo.text;
    if (texto.isEmpty || _enviando) return;
    setState(() {
      _enviando = true;
      _falhaDoEnvio = null;
    });
    final falha = await widget.plantao.enviar(widget.codigo, texto);
    if (!mounted) return;
    setState(() {
      _enviando = false;
      _falhaDoEnvio = falha;
      // Só limpa quando foi: com falha, o texto fica para tentar de novo.
      if (falha == null) _campo.clear();
    });
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: widget.plantao,
      builder: (context, _) {
        final linhas = _linhas();
        final falhaDaCaixa = widget.plantao.falhaDaCaixa;
        return AnnotatedRegion(
          value: estiloBarras,
          child: Scaffold(
            body: SafeArea(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  _Cabecalho(codigo: widget.codigo),
                  Expanded(
                    child: linhas.isEmpty
                        ? const Padding(
                            padding: EdgeInsets.all(18),
                            child: Text(
                              'Nenhuma mensagem. As recebidas ficam seladas até '
                              'você segurar; as enviadas queimam 60 s depois de '
                              'enviadas.',
                              key: Key('conversa-vazia'),
                              style: estiloTextoCorrido,
                            ),
                          )
                        : ListView(
                            key: const Key('mensagens'),
                            padding: const EdgeInsets.all(18),
                            children: [
                              for (var i = 0; i < linhas.length; i++)
                                _Mensagem(
                                  key: Key('msg-${linhas[i].id}'),
                                  numero: i + 1,
                                  linha: linhas[i],
                                  plantao: widget.plantao,
                                  texto: _segurando == linhas[i].id
                                      ? _texto
                                      : null,
                                  aoSegurar: () => _segurar(linhas[i]),
                                  aoSoltar: _soltar,
                                ),
                            ],
                          ),
                  ),
                  if (falhaDaCaixa != null)
                    Padding(
                      padding: const EdgeInsets.fromLTRB(18, 0, 18, 8),
                      child: Text(
                        'Falha na caixa ($falhaDaCaixa).',
                        key: const Key('aviso-caixa'),
                        style: estiloAviso,
                      ),
                    ),
                  const Padding(
                    padding: EdgeInsets.fromLTRB(18, 0, 18, 12),
                    child: Row(
                      children: [
                        SizedBox(
                          width: 8,
                          height: 8,
                          child: ColoredBox(color: Cores.laranja),
                        ),
                        SizedBox(width: 10),
                        Expanded(
                          child: Text(
                            'SEGURE PARA DECIFRAR · A PÁGINA QUEIMA APÓS O USO',
                            style: TextStyle(
                              fontSize: 11,
                              letterSpacing: 1,
                              color: Cores.secundario,
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                  _Rodape(
                    podeEnviar: widget.podeEnviar,
                    motivoSemEnvio: widget.motivoSemEnvio,
                    campo: _campo,
                    enviando: _enviando,
                    falha: _falhaDoEnvio,
                    aoCifrar: _cifrar,
                  ),
                ],
              ),
            ),
          ),
        );
      },
    );
  }
}

class _Cabecalho extends StatelessWidget {
  const _Cabecalho({required this.codigo});

  final String codigo;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.fromLTRB(6, 14, 18, 12),
      decoration: const BoxDecoration(
        border: Border(bottom: BorderSide(color: Cores.ambar, width: 2)),
      ),
      child: Row(
        children: [
          Semantics(
            button: true,
            label: 'Voltar à central',
            excludeSemantics: true,
            child: InkWell(
              key: const Key('voltar-central'),
              onTap: () => Navigator.of(context).maybePop(),
              child: const SizedBox(
                width: 48,
                height: 48,
                child: Icon(Icons.chevron_left, color: Cores.ambar, size: 24),
              ),
            ),
          ),
          const SizedBox(width: 6),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text('MODO 02 · CONVERSA', style: estiloRotulo),
                const SizedBox(height: 4),
                Text(
                  codigo.isEmpty ? 'SEM REMETENTE' : codigo,
                  key: const Key('codigo-conversa'),
                  style: const TextStyle(
                    fontSize: 17,
                    fontWeight: FontWeight.w700,
                    letterSpacing: 1,
                    color: Cores.ambar,
                  ),
                ),
              ],
            ),
          ),
          const Text(
            'QUEIMA 60 S',
            style: TextStyle(
              fontSize: 10,
              letterSpacing: 1,
              color: Cores.laranja,
            ),
          ),
        ],
      ),
    );
  }
}

class _Mensagem extends StatelessWidget {
  const _Mensagem({
    super.key,
    required this.numero,
    required this.linha,
    required this.plantao,
    required this.texto,
    required this.aoSegurar,
    required this.aoSoltar,
  });

  final int numero;
  final _Linha linha;
  final Plantao plantao;
  final String? texto;
  final VoidCallback aoSegurar;
  final VoidCallback aoSoltar;

  static String _duasCasas(int n) => n.toString().padLeft(2, '0');

  static String _relogio(int segundos) =>
      '${_duasCasas(segundos ~/ 60)}:${_duasCasas(segundos % 60)}';

  /// Letras de enfeite, as mesmas a cada desenho da mesma mensagem. Não vêm do texto.
  static String _grupos(String id) {
    final sorteio = Random(id.hashCode);
    return [
      for (var g = 0; g < gruposFechados; g++)
        String.fromCharCodes([
          for (var l = 0; l < 5; l++) 0x41 + sorteio.nextInt(26),
        ]),
    ].join(' ');
  }

  @override
  Widget build(BuildContext context) {
    final selada = linha.selada;
    final enviada = linha.enviada;
    final hora = '${_duasCasas(linha.em.hour)}:${_duasCasas(linha.em.minute)}';
    final sentido = enviada != null ? 'TX' : 'RX';
    final String meta;
    final bool queimando;
    if (enviada != null) {
      meta =
          'ENVIADA · QUEIMA EM ${_relogio(plantao.segundosAteQueimar(enviada.em))}';
      queimando = true;
    } else if (selada!.lida) {
      final desde = plantao.lidaEm(selada.gaveta);
      meta = desde == null
          ? 'LIDA · QUEIMA AO SAIR'
          : 'LIDA · QUEIMA EM ${_relogio(plantao.segundosAteQueimar(desde))}';
      queimando = true;
    } else {
      meta = '$gruposFechados GRUPOS';
      queimando = false;
    }
    final aberta = texto != null;
    return Padding(
      padding: const EdgeInsets.only(bottom: 20),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  'MSG ${_duasCasas(numero)} · $sentido $hora',
                  style: const TextStyle(
                    fontSize: 11,
                    letterSpacing: 1,
                    color: Cores.secundario,
                  ),
                ),
              ),
              Text(
                meta,
                key: Key('meta-${linha.id}'),
                style: TextStyle(
                  fontSize: 11,
                  letterSpacing: 1,
                  color: queimando ? Cores.laranja : Cores.secundario,
                ),
              ),
            ],
          ),
          const SizedBox(height: 8),
          // O Listener vê o dedo sair mesmo depois de o gesto de segurar ter sido
          // aceito (o GestureDetector não avisa o cancelamento nesse caso).
          Listener(
            onPointerUp: (_) => aoSoltar(),
            onPointerCancel: (_) => aoSoltar(),
            child: Semantics(
              label: aberta ? null : 'Mensagem fechada. Segure para decifrar.',
              child: GestureDetector(
                key: Key('segurar-${linha.id}'),
                onLongPressStart: (_) => aoSegurar(),
                onLongPressEnd: (_) => aoSoltar(),
                child: Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 14,
                    vertical: 12,
                  ),
                  decoration: BoxDecoration(
                    color: Cores.painel,
                    border: Border.all(
                      color: aberta ? Cores.ambar : Cores.linha,
                    ),
                  ),
                  child: aberta
                      ? Text(
                          texto!,
                          key: Key('texto-${linha.id}'),
                          style: const TextStyle(
                            fontSize: 14,
                            height: 1.6,
                            color: Cores.texto,
                          ),
                        )
                      : Text(
                          _grupos(linha.id),
                          style: const TextStyle(
                            fontSize: 14,
                            height: 1.8,
                            letterSpacing: 2,
                            wordSpacing: 6,
                            color: Cores.ambar,
                          ),
                        ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _Rodape extends StatelessWidget {
  const _Rodape({
    required this.podeEnviar,
    required this.motivoSemEnvio,
    required this.campo,
    required this.enviando,
    required this.falha,
    required this.aoCifrar,
  });

  final bool podeEnviar;
  final String motivoSemEnvio;
  final TextEditingController campo;
  final bool enviando;
  final String? falha;
  final VoidCallback aoCifrar;

  @override
  Widget build(BuildContext context) {
    final f = falha;
    return Container(
      padding: const EdgeInsets.fromLTRB(18, 12, 18, 16),
      decoration: const BoxDecoration(
        border: Border(top: BorderSide(color: Cores.linha)),
      ),
      child: !podeEnviar
          ? Text(
              motivoSemEnvio,
              key: const Key('sem-envio'),
              style: estiloTextoCorrido,
            )
          : Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Row(
                  crossAxisAlignment: CrossAxisAlignment.end,
                  children: [
                    Expanded(
                      child: TextField(
                        key: const Key('campo-texto'),
                        controller: campo,
                        minLines: 1,
                        maxLines: 4,
                        // Nada de sugestão, correção nem aprendizado do teclado:
                        // o texto em claro não vai para o dicionário do teclado.
                        enableSuggestions: false,
                        autocorrect: false,
                        enableIMEPersonalizedLearning: false,
                        keyboardAppearance: Brightness.dark,
                        textCapitalization: TextCapitalization.sentences,
                        style: const TextStyle(
                          fontSize: 14,
                          color: Cores.texto,
                        ),
                        cursorColor: Cores.ambar,
                        decoration: const InputDecoration(
                          hintText: 'Texto em claro',
                          hintStyle: TextStyle(color: Cores.secundario),
                          filled: true,
                          fillColor: Cores.painel,
                          isDense: true,
                          contentPadding: EdgeInsets.symmetric(
                            horizontal: 14,
                            vertical: 14,
                          ),
                          enabledBorder: OutlineInputBorder(
                            borderRadius: BorderRadius.zero,
                            borderSide: BorderSide(color: Cores.linha),
                          ),
                          focusedBorder: OutlineInputBorder(
                            borderRadius: BorderRadius.zero,
                            borderSide: BorderSide(color: Cores.ambar),
                          ),
                        ),
                      ),
                    ),
                    const SizedBox(width: 10),
                    SizedBox(
                      width: 112,
                      child: Botao(
                        nome: 'CIFRAR',
                        rotulo: enviando ? 'CIFRANDO…' : 'CIFRAR',
                        aoTocar: enviando || campo.text.isEmpty
                            ? null
                            : aoCifrar,
                      ),
                    ),
                  ],
                ),
                if (f != null) ...[
                  const SizedBox(height: 8),
                  Text(
                    mensagemDoEnvio(f),
                    key: const Key('aviso-envio'),
                    style: estiloAviso,
                  ),
                ],
              ],
            ),
    );
  }
}

/// Texto do aviso quando o envio falha. Laranja na tela: é alerta.
String mensagemDoEnvio(String codigo) {
  if (codigo.startsWith('REDE_')) {
    return 'Sem conexão com o relay. O texto continua no campo.';
  }
  return switch (codigo) {
    'TEXTO_GRANDE' => 'Texto maior que 65.536 bytes. Divida em partes.',
    'SEM_CHAVE' =>
      'O contato não tem chave no relay: ele precisa abrir o app uma vez.',
    'SEM_RELAY' => 'App montado sem relay: não há para onde mandar.',
    'MALOTE_CHEIO' => 'O relay está cheio para este contato. Tente mais tarde.',
    _ => 'Não foi possível enviar ($codigo). O texto continua no campo.',
  };
}
