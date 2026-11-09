// Tela da sonda BLE do modo Direto (documentacao/arquitetura/modo-direto.md, passo 1).
// Só existe no app montado com --dart-define=KRYPTOS_SONDA_BLE=true, e o lado Android
// (SondaBle.kt) só existe no app de debug. Sem núcleo, sem PIN, sem protocolo do Kryptos:
// a sonda manda bytes aleatórios e mede.
//
// Tudo o que a sonda mede vai para o logcat (tag KryptosSonda) e para um arquivo na pasta
// privada do app, que o ferramentas/instalar-no-celular.ps1 -SondaBle lê no fim. Aqui a
// tela só escolhe o papel e as variáveis e mostra as linhas.
//
// Roteiro de alcance (carregado pelo script antes de os celulares saírem do cabo): o
// Android manda "roteiro" com o texto da vez; quando ele espera a pessoa, a tela mostra
// PRÓXIMO PONTO.

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'tema.dart';

const canalSondaBle = MethodChannel('kryptos/sonda-ble');

class SondaBleApp extends StatelessWidget {
  const SondaBleApp({super.key, this.canal = canalSondaBle});

  final MethodChannel canal;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Kryptos · sonda BLE',
      debugShowCheckedModeBanner: false,
      theme: temaKryptos(),
      home: TelaSondaBle(canal: canal),
    );
  }
}

class TelaSondaBle extends StatefulWidget {
  const TelaSondaBle({super.key, required this.canal});

  final MethodChannel canal;

  @override
  State<TelaSondaBle> createState() => _TelaSondaBleState();
}

class _TelaSondaBleState extends State<TelaSondaBle> {
  static const _maxLinhas = 300;

  bool? _permitido;
  bool _ocupado = false;
  String _inventario = '';
  String? _erro;
  String _phy = '1M';
  String _prioridade = 'PADRAO';
  // Caminho da volta (servidor -> cliente): NOTIFICACAO, ESCRITA (servidor escreve no
  // cliente pelo mesmo enlace) ou L2CAP. Quem escolhe é o celular que procura.
  String _volta = 'NOTIFICACAO';
  int _distancia = 1;
  String _roteiro = '';
  bool _roteiroAguardando = false;
  final _linhas = <String>[];

  @override
  void initState() {
    super.initState();
    widget.canal.setMethodCallHandler(_doAndroid);
    _pedirPermissao();
  }

  @override
  void dispose() {
    widget.canal.setMethodCallHandler(null);
    super.dispose();
  }

  Future<dynamic> _doAndroid(MethodCall c) async {
    if (!mounted) return null;
    switch (c.method) {
      case 'linha':
        setState(() {
          _linhas.insert(0, c.arguments as String);
          if (_linhas.length > _maxLinhas) _linhas.removeLast();
        });
      case 'ocupado':
        setState(() => _ocupado = c.arguments as bool);
      case 'roteiro':
        final m = (c.arguments as Map).cast<String, Object?>();
        setState(() {
          _roteiro = m['texto'] as String? ?? '';
          _roteiroAguardando = m['aguardando'] == true;
        });
    }
    return null;
  }

  Future<void> _pedirPermissao() async {
    try {
      final ok = await widget.canal.invokeMethod<bool>('permissoes') ?? false;
      if (!mounted) return;
      setState(() => _permitido = ok);
      if (!ok) return;
      final inv = await widget.canal.invokeMethod<String>('inventario') ?? '';
      if (mounted) setState(() => _inventario = inv);
    } on PlatformException catch (e) {
      if (mounted) setState(() => _erro = e.code);
    }
  }

  Future<void> _chamar(String metodo, [Map<String, Object?>? args]) async {
    setState(() => _erro = null);
    try {
      await widget.canal.invokeMethod<void>(metodo, args);
    } on PlatformException catch (e) {
      if (mounted) setState(() => _erro = e.code);
    }
  }

  Map<String, Object?> _config({bool calibrar = false}) => {
    'phy': _phy,
    'prioridade': _prioridade,
    'distancia': _distancia,
    'volta': _volta,
    'repeticoes': calibrar ? 3 : 5,
    'calibrar': calibrar,
  };

  @override
  Widget build(BuildContext context) {
    final livre = _permitido == true && !_ocupado;
    return Scaffold(
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(12),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              const Text('SONDA BLE · SÓ TESTE', style: estiloRotulo),
              const SizedBox(height: 6),
              if (_permitido == false)
                Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    const Text(
                      'SEM PERMISSÃO DE DISPOSITIVOS POR PERTO',
                      key: Key('sonda-sem-permissao'),
                      style: TextStyle(color: Cores.laranja),
                    ),
                    _botao(
                      'TENTAR DE NOVO',
                      'sonda-tentar',
                      true,
                      _pedirPermissao,
                    ),
                  ],
                ),
              if (_inventario.isNotEmpty)
                Text(
                  _inventario,
                  key: const Key('sonda-inventario'),
                  style: const TextStyle(fontSize: 10, color: Cores.secundario),
                ),
              if (_erro != null)
                Text(
                  'ERRO $_erro',
                  key: const Key('sonda-erro'),
                  style: const TextStyle(color: Cores.laranja),
                ),
              if (_roteiro.isNotEmpty)
                Container(
                  key: const Key('sonda-roteiro'),
                  margin: const EdgeInsets.only(top: 8),
                  padding: const EdgeInsets.all(10),
                  decoration: BoxDecoration(
                    border: Border.all(color: Cores.ambar),
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Text(_roteiro, style: const TextStyle(fontSize: 14)),
                      if (_roteiroAguardando) ...[
                        const SizedBox(height: 8),
                        FilledButton(
                          key: const Key('sonda-proximo-ponto'),
                          onPressed: () => _chamar('proximo'),
                          child: const Text('PRÓXIMO PONTO'),
                        ),
                      ],
                    ],
                  ),
                ),
              const SizedBox(height: 8),
              _escolha(
                'phy',
                'PHY',
                ['1M', '2M', 'CODED'],
                _phy,
                (v) => _phy = v,
              ),
              _escolha(
                'prioridade',
                'PRIORIDADE',
                ['PADRAO', 'ALTA'],
                _prioridade,
                (v) => _prioridade = v,
              ),
              _escolha(
                'volta',
                'VOLTA',
                ['NOTIFICACAO', 'ESCRITA', 'L2CAP'],
                _volta,
                (v) => _volta = v,
              ),
              _escolha(
                'distancia',
                'DISTÂNCIA (m)',
                ['1', '5', '10', '15', '20', '30'],
                '$_distancia',
                (v) => _distancia = int.parse(v),
              ),
              const SizedBox(height: 6),
              Wrap(
                spacing: 6,
                runSpacing: 6,
                children: [
                  _botao(
                    'ANUNCIAR',
                    'sonda-anunciar',
                    livre,
                    () => _chamar('anunciar'),
                  ),
                  _botao(
                    'PROCURAR',
                    'sonda-procurar',
                    livre,
                    () => _chamar('procurar', _config()),
                  ),
                  _botao(
                    'CALIBRAR',
                    'sonda-calibrar',
                    livre,
                    () => _chamar('procurar', _config(calibrar: true)),
                  ),
                  _botao(
                    'AMBOS',
                    'sonda-ambos',
                    livre,
                    () => _chamar('ambos', _config()),
                  ),
                  _botao(
                    'PARAR',
                    'sonda-parar',
                    _permitido == true,
                    () => _chamar('parar'),
                  ),
                  // Abre o "Compartilhar" do Android com o arquivo de medidas inteiro, em
                  // texto. Só com a sonda parada: abrir a escolha tira o app da frente.
                  _botao(
                    'ENVIAR MEDIDAS',
                    'sonda-compartilhar',
                    !_ocupado,
                    () => _chamar('compartilhar'),
                  ),
                ],
              ),
              const SizedBox(height: 8),
              Text(
                _ocupado ? 'EM ANDAMENTO' : 'PARADA',
                key: const Key('sonda-estado'),
                style: estiloRotulo,
              ),
              const Divider(color: Cores.linha),
              Expanded(
                child: ListView.builder(
                  key: const Key('sonda-linhas'),
                  itemCount: _linhas.length,
                  itemBuilder: (_, i) => Text(
                    _linhas[i],
                    style: TextStyle(
                      fontSize: 10,
                      color: _linhas[i].contains('FALHA')
                          ? Cores.laranja
                          : Cores.textoCorrido,
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _escolha(
    String chave,
    String rotulo,
    List<String> opcoes,
    String atual,
    void Function(String) mudar,
  ) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 4),
      child: Wrap(
        spacing: 6,
        crossAxisAlignment: WrapCrossAlignment.center,
        children: [
          SizedBox(width: 110, child: Text(rotulo, style: estiloRotulo)),
          for (final o in opcoes)
            ChoiceChip(
              key: Key('sonda-$chave-$o'),
              label: Text(o, style: const TextStyle(fontSize: 11)),
              selected: o == atual,
              onSelected: _ocupado ? null : (_) => setState(() => mudar(o)),
            ),
        ],
      ),
    );
  }

  Widget _botao(String rotulo, String chave, bool ativo, VoidCallback acao) {
    return OutlinedButton(
      key: Key(chave),
      onPressed: ativo ? acao : null,
      child: Text(rotulo),
    );
  }
}
