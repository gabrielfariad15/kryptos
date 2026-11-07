// Novo contato por QR (tela "novo contato" aprovada, aba QR).
//
// Três momentos:
//   - MEU QR: o QR deste aparelho e o código. LER QR DO CONTATO liga a câmera.
//   - LENDO: a câmera no lugar do QR; a leitura é automática, um quadro por vez.
//     QR recusado (de outro app, adulterado, o próprio) avisa em laranja e a leitura
//     continua. CANCELAR, voltar ou o app ir para o fundo desligam a câmera.
//   - LIDO: o contato entrou na agenda com a chave conferida por QR. Para o outro lado
//     ficar conferido também, ele lê o QR deste aparelho (MOSTRAR MEU QR).
//
// As abas CÓDIGO e SOM do visual aprovado ficam para quando houver como trocar o
// contato por elas: aqui só aparece o que funciona (regra 5.5).

import 'dart:async';

import 'package:flutter/material.dart';

import '../contatos.dart';
import '../leitor_de_qr.dart';
import '../tema.dart';
import 'componentes.dart';
import 'qr_desenhado.dart';

enum _Modo { meuQr, lendo, lido }

class NovoContato extends StatefulWidget {
  const NovoContato({
    super.key,
    required this.versaoNucleo,
    required this.codigo,
    required this.agenda,
    required this.abrirCamera,
  });

  final String versaoNucleo;

  /// Código deste aparelho (XXXX-XXXX).
  final String codigo;
  final Agenda agenda;
  final AbrirCamera abrirCamera;

  @override
  State<NovoContato> createState() => _NovoContatoState();
}

class _NovoContatoState extends State<NovoContato> with WidgetsBindingObserver {
  _Modo _modo = _Modo.meuQr;
  GradeQr? _grade;
  String? _avisoDaGrade;

  Camera? _camera;
  bool _cameraPronta = false;
  LeitorDeQr? _leitor;

  /// Muda a cada leitura começada ou encerrada: uma câmera que termina de abrir
  /// depois do CANCELAR sabe que não é mais a dela.
  int _sessao = 0;
  String? _aviso;

  ContatoLido? _lido;
  MedidaDaLeitura? _medida;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    try {
      _grade = widget.agenda.gradePropria();
      if (_grade == null) _avisoDaGrade = mensagemDaRecusa('SEM_IDENTIDADE');
    } on QrRecusado catch (e) {
      debugPrint('Kryptos: falha ao montar o QR próprio (${e.codigo})');
      _avisoDaGrade = 'Não foi possível montar o seu QR (${e.codigo}).';
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState estado) {
    // Só quando o app sai da tela. O "inactive" também chega com o pedido de
    // permissão da câmera aberto por cima, e aí a leitura tem de continuar.
    if (_modo == _Modo.lendo &&
        (estado == AppLifecycleState.hidden ||
            estado == AppLifecycleState.paused)) {
      unawaited(_encerrarLeitura(desfecho: 'cancelado (app saiu da tela)'));
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _sessao++;
    final medida = _leitor?.parar();
    if (_modo == _Modo.lendo && medida != null) {
      debugPrint(medida.linha('cancelado (saiu da tela)'));
    }
    final camera = _camera;
    _camera = null;
    if (camera != null) unawaited(_fecharSemFalha(camera));
    super.dispose();
  }

  Future<void> _lerQr() async {
    if (_modo == _Modo.lendo || _grade == null) return;
    final sessao = ++_sessao;
    final camera = widget.abrirCamera();
    setState(() {
      _modo = _Modo.lendo;
      _camera = camera;
      _cameraPronta = false;
      _aviso = null;
      _lido = null;
      _medida = null;
    });
    try {
      await camera.abrir();
      if (!mounted || sessao != _sessao) {
        await _fecharSemFalha(camera);
        return;
      }
      final leitor = LeitorDeQr(
        ler: widget.agenda.lerQuadro,
        aoAchar: (contato, medida) => _achou(sessao, contato, medida),
        aoRecusar: (codigo) => _recusou(sessao, codigo),
      );
      setState(() {
        _cameraPronta = true;
        _leitor = leitor;
      });
      await camera.receberQuadros(leitor.receber);
    } on FalhaDaCamera catch (e) {
      debugPrint('Kryptos: câmera não abriu (${e.codigo})');
      await _fecharSemFalha(camera);
      if (!mounted || sessao != _sessao) return;
      setState(() {
        _modo = _Modo.meuQr;
        _camera = null;
        _aviso = mensagemDaCamera(e.codigo);
      });
    }
  }

  void _recusou(int sessao, String codigo) {
    if (!mounted || sessao != _sessao) return;
    final texto = mensagemDaRecusa(codigo);
    if (texto != _aviso) setState(() => _aviso = texto);
  }

  Future<void> _achou(
    int sessao,
    ContatoLido contato,
    MedidaDaLeitura medida,
  ) async {
    if (!mounted || sessao != _sessao) return;
    debugPrint(medida.linha(contato.novo ? 'lido (novo)' : 'lido (ja estava)'));
    _sessao++;
    final camera = _camera;
    setState(() {
      _modo = _Modo.lido;
      _lido = contato;
      _medida = medida;
      _aviso = null;
      _camera = null;
      _cameraPronta = false;
      _leitor = null;
    });
    if (camera != null) await _fecharSemFalha(camera);
  }

  Future<void> _encerrarLeitura({required String desfecho}) async {
    if (_modo != _Modo.lendo) return;
    _sessao++;
    final medida = _leitor?.parar();
    if (medida != null) debugPrint(medida.linha(desfecho));
    final camera = _camera;
    setState(() {
      _modo = _Modo.meuQr;
      _camera = null;
      _cameraPronta = false;
      _leitor = null;
      _aviso = null;
    });
    if (camera != null) await _fecharSemFalha(camera);
  }

  void _mostrarMeuQr() => setState(() {
    _modo = _Modo.meuQr;
    _lido = null;
    _medida = null;
  });

  static Future<void> _fecharSemFalha(Camera camera) async {
    try {
      await camera.fechar();
    } catch (e) {
      debugPrint('Kryptos: falha ao fechar a câmera (${e.runtimeType})');
    }
  }

  @override
  Widget build(BuildContext context) {
    final conteudo = switch (_modo) {
      _Modo.meuQr => _meuQr(),
      _Modo.lendo => _lendo(),
      _Modo.lido => _contatoLido(),
    };
    final acao = switch (_modo) {
      _Modo.meuQr => Botao(
        nome: 'LER',
        rotulo: 'LER QR DO CONTATO',
        aoTocar: _grade == null ? null : _lerQr,
      ),
      _Modo.lendo => Botao(
        nome: 'CANCELAR',
        rotulo: 'CANCELAR',
        aoTocar: () => _encerrarLeitura(desfecho: 'cancelado'),
      ),
      _Modo.lido => Botao(
        nome: 'MOSTRAR',
        rotulo: 'MOSTRAR MEU QR',
        aoTocar: _mostrarMeuQr,
      ),
    };
    return AnnotatedRegion(
      value: estiloBarras,
      child: Scaffold(
        body: SafeArea(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              const _CabecalhoDeVolta(),
              Expanded(
                child: SingleChildScrollView(
                  padding: const EdgeInsets.fromLTRB(18, 14, 18, 12),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      ...conteudo,
                      const SizedBox(height: 14),
                      const _DepoisDaTroca(),
                    ],
                  ),
                ),
              ),
              Padding(
                padding: const EdgeInsets.fromLTRB(18, 10, 18, 10),
                child: acao,
              ),
              Rodape(versaoNucleo: widget.versaoNucleo),
            ],
          ),
        ),
      ),
    );
  }

  List<Widget> _meuQr() {
    final grade = _grade;
    return [
      const Text('01 · SEU QR', style: estiloRotulo),
      const SizedBox(height: 10),
      if (grade != null)
        QrDesenhado(grade: grade)
      else
        Aviso(_avisoDaGrade ?? mensagemDaRecusa('SEM_IDENTIDADE')),
      const SizedBox(height: 14),
      const Center(child: Text('SEU CÓDIGO', style: estiloRotulo)),
      const SizedBox(height: 4),
      Center(
        child: Text(
          widget.codigo,
          key: const Key('codigo-proprio'),
          style: const TextStyle(
            fontSize: 22,
            fontWeight: FontWeight.w700,
            letterSpacing: 4,
            color: Cores.ambar,
          ),
        ),
      ),
      const SizedBox(height: 8),
      if (_aviso != null)
        Aviso(_aviso!)
      else
        const Text(
          'Mostre este QR para o contato ler, ou toque abaixo para ler o dele.',
          style: estiloTextoCorrido,
          textAlign: TextAlign.center,
        ),
    ];
  }

  List<Widget> _lendo() {
    final camera = _camera;
    return [
      const Text('01 · QR DO CONTATO', style: estiloRotulo),
      const SizedBox(height: 10),
      AspectRatio(
        aspectRatio: 1,
        child: Container(
          key: const Key('camera'),
          decoration: BoxDecoration(
            color: Cores.painel,
            border: Border.all(color: Cores.ambar, width: 2),
          ),
          clipBehavior: Clip.hardEdge,
          alignment: Alignment.center,
          child: _cameraPronta && camera != null
              ? SizedBox.expand(child: camera.previa())
              : const Text('ABRINDO A CÂMERA…', style: estiloRotulo),
        ),
      ),
      const SizedBox(height: 8),
      if (_aviso != null)
        Aviso(_aviso!)
      else
        const Text(
          'Aponte para o QR na tela do contato. A leitura é automática.',
          style: estiloTextoCorrido,
        ),
    ];
  }

  List<Widget> _contatoLido() {
    final lido = _lido!;
    final medida = _medida;
    final segundos = medida == null
        ? ''
        : ' Lido em ${(medida.tempo.inMilliseconds / 1000).toStringAsFixed(1).replaceAll('.', ',')} s.';
    return [
      const Text('01 · CONTATO', style: estiloRotulo),
      const SizedBox(height: 10),
      Container(
        key: const Key('contato-lido'),
        padding: const EdgeInsets.all(14),
        decoration: BoxDecoration(
          color: Cores.painel,
          border: Border.all(color: Cores.linha),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              lido.novo ? 'CONTATO ADICIONADO' : 'JÁ ESTAVA NA AGENDA',
              style: estiloRotulo,
            ),
            const SizedBox(height: 6),
            Text(
              lido.codigo,
              key: const Key('codigo-lido'),
              style: const TextStyle(
                fontSize: 22,
                fontWeight: FontWeight.w700,
                letterSpacing: 4,
                color: Cores.texto,
              ),
            ),
            const SizedBox(height: 6),
            const Text(
              'CHAVE CONFERIDA · LIDA POR QR',
              key: Key('chave-conferida'),
              style: TextStyle(
                fontSize: 11,
                fontWeight: FontWeight.w700,
                letterSpacing: 1,
                color: Cores.ambar,
              ),
            ),
            const SizedBox(height: 6),
            Text(
              'Confira se é o código que aparece na tela do contato.$segundos',
              style: estiloTextoCorrido,
            ),
          ],
        ),
      ),
      const SizedBox(height: 10),
      const Text(
        'Para o contato também conferir a sua chave, mostre o seu QR para ele ler.',
        style: estiloTextoCorrido,
      ),
    ];
  }
}

class _CabecalhoDeVolta extends StatelessWidget {
  const _CabecalhoDeVolta();

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
            label: 'Voltar',
            excludeSemantics: true,
            child: InkWell(
              key: const Key('voltar'),
              onTap: () => Navigator.of(context).maybePop(),
              child: const SizedBox(
                width: 48,
                height: 48,
                child: Icon(Icons.chevron_left, color: Cores.ambar, size: 26),
              ),
            ),
          ),
          const SizedBox(width: 6),
          const Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('COFRE · NOVO CONTATO', style: estiloRotulo),
                SizedBox(height: 4),
                Text(
                  'TROCA DE CÓDIGO',
                  style: TextStyle(
                    fontSize: 17,
                    fontWeight: FontWeight.w700,
                    letterSpacing: 1,
                    color: Cores.ambar,
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _DepoisDaTroca extends StatelessWidget {
  const _DepoisDaTroca();

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(border: Border.all(color: Cores.laranja)),
      child: const Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            'DEPOIS DA TROCA',
            style: TextStyle(
              fontSize: 11,
              fontWeight: FontWeight.w700,
              letterSpacing: 1,
              color: Cores.laranja,
            ),
          ),
          SizedBox(height: 6),
          Text(
            'Ler o QR confere a chave num sentido só. Para os dois lados ficarem conferidos, cada aparelho lê o QR do outro. Nunca aceite um QR que chegou por outro app.',
            style: estiloTextoCorrido,
          ),
        ],
      ),
    );
  }
}
