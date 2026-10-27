import prelude

import logging
import socket
import torch
import numpy as np
import time
import gc
import glob
import sys
import os
from os import path
from collections import deque
from model import Brain, DQN
from player import TrainPlayer
from common import send_msg, recv_msg
from config import config

def promote(mortal, dqn, param_version):
    """Snapshot the current trainee params into the champion pool when the
    rolling per-game CU gate passes (see fsp_gate config)."""
    champion_dir = config['fsp_pool']['champion_dir']
    os.makedirs(champion_dir, exist_ok=True)
    fname = f'{int(time.time())}_pv{param_version}.pth'
    tmp = path.join(champion_dir, f'.{fname}.tmp')
    torch.save({
        'mortal': {k: v.detach().cpu() for k, v in mortal.state_dict().items()},
        'current_dqn': {k: v.detach().cpu() for k, v in dqn.state_dict().items()},
        'config': config,
    }, tmp)
    os.replace(tmp, path.join(champion_dir, fname))

    cap = gate_cfg.get('pool_cap', 10)
    files = sorted(glob.glob(path.join(champion_dir, '*.pth')), key=os.path.getmtime)
    for f in files[:-cap]:
        os.remove(f)
    logging.info(f'PROMOTED {fname} (pool size {min(len(files), cap)})')

gate_cfg = config.get('fsp_gate', {})
cu_window = deque(maxlen=gate_cfg.get('window', 1000))
# per-promotion-cycle state; drives the decaying threshold and the
# convergence decision
gate_state = {
    'base_version': 0,      # param_version at cycle start
    'failures': 0,          # failed full-window evaluations after grace
}

def current_threshold(param_version):
    """Threshold stays at the initial value for the first `grace_steps`
    training steps of a promotion cycle (1000 hanchans may be far too few
    for anything to be learned yet), then decays per failed evaluation.
    Training steps are estimated as param_version delta * submit_every."""
    submit_every = config['control']['submit_every']
    steps_in_cycle = (param_version - gate_state['base_version']) * submit_every
    if steps_in_cycle < gate_cfg.get('grace_steps', 100_000):
        return gate_cfg.get('initial_threshold', 3.0)
    return max(
        gate_cfg.get('floor', 0.5),
        gate_cfg.get('initial_threshold', 3.0)
        * gate_cfg.get('decay', 0.85) ** gate_state['failures'],
    )

def gate_check(cus, param_version):
    """Feed per-game challenger CUs into the rolling window. Every time the
    window fills (1000 hanchans), one evaluation happens: promote when the
    lower confidence bound of the window mean exceeds the current threshold.
    The window is conservative (params improve within a window, so the mean
    understates the current version's edge). After `grace_steps` training
    steps without promotion, the threshold decays towards the floor; after
    `patience` more consecutive failures it has reached the floor and the run
    is considered converged — the client exits and drops a marker file.
    """
    if not cus:
        return False
    cu_window.extend(cus)
    if len(cu_window) < cu_window.maxlen:
        return False

    arr = np.asarray(cu_window)
    cu_window.clear()
    se = arr.std(ddof=1) / np.sqrt(len(arr))
    lb = arr.mean() - gate_cfg.get('z', 1.64) * se
    th = current_threshold(param_version)
    logging.info(f'gate: n={len(arr)} avg={arr.mean():.3f} se={se:.3f} '
                 f'lb={lb:.3f} th={th:.3f} failures={gate_state["failures"]}')

    if lb > th:
        gate_state['base_version'] = param_version
        gate_state['failures'] = 0
        return True

    submit_every = config['control']['submit_every']
    steps_in_cycle = (param_version - gate_state['base_version']) * submit_every
    if steps_in_cycle >= gate_cfg.get('grace_steps', 100_000):
        gate_state['failures'] += 1
    patience = gate_cfg.get('patience', 30)
    if gate_state['failures'] >= patience:
        with open(gate_cfg.get('converged_marker', '/tmp/CONVERGED'), 'w') as f:
            f.write(f'converged after {gate_state["failures"]} post-grace failures\n')
        logging.info(f'CONVERGED after {gate_state["failures"]} failed gate evaluations, exiting')
        sys.exit(0)
    return False

def main():
    remote = (config['online']['remote']['host'], config['online']['remote']['port'])
    device = torch.device(config['control']['device'])
    version = config['control']['version']
    num_blocks = config['resnet']['num_blocks']
    conv_channels = config['resnet']['conv_channels']

    mortal = Brain(version=version, num_blocks=num_blocks, conv_channels=conv_channels).to(device).eval()
    dqn = DQN(version=version).to(device)
    if config['online']['enable_compile']:
        mortal.compile()
        dqn.compile()

    train_player = TrainPlayer()
    param_version = -1

    pts = np.array([90, 45, 0, -135])
    history_window = config['online']['history_window']
    history = []

    while True:
        while True:
            with socket.socket() as conn:
                conn.connect(remote)
                msg = {
                    'type': 'get_param',
                    'param_version': param_version,
                }
                send_msg(conn, msg)
                rsp = recv_msg(conn, map_location=device)
                if rsp['status'] == 'ok':
                    param_version = rsp['param_version']
                    break
                time.sleep(3)
        mortal.load_state_dict(rsp['mortal'])
        dqn.load_state_dict(rsp['dqn'])
        logging.info('param has been updated')

        rankings, file_list = train_player.train_play(mortal, dqn, device)
        avg_rank = rankings @ np.arange(1, 5) / rankings.sum()
        avg_pt = rankings @ pts / rankings.sum()

        if gate_check(getattr(train_player, 'last_game_cus', []), param_version):
            promote(mortal, dqn, param_version)

        history.append(np.array(rankings))
        if len(history) > history_window:
            del history[0]
        sum_rankings = np.sum(history, axis=0)
        ma_avg_rank = sum_rankings @ np.arange(1, 5) / sum_rankings.sum()
        ma_avg_pt = sum_rankings @ pts / sum_rankings.sum()

        logging.info(f'trainee rankings: {rankings} ({avg_rank:.6}, {avg_pt:.6}pt)')
        logging.info(f'last {len(history)} sessions: {sum_rankings} ({ma_avg_rank:.6}, {ma_avg_pt:.6}pt)')

        logs = {}
        for filename in file_list:
            with open(filename, 'rb') as f:
                logs[path.basename(filename)] = f.read()

        with socket.socket() as conn:
            conn.connect(remote)
            send_msg(conn, {
                'type': 'submit_replay',
                'logs': logs,
                'param_version': param_version,
            })
            logging.info('logs have been submitted')
        gc.collect()
        torch.cuda.empty_cache()
        torch.cuda.synchronize()

if __name__ == '__main__':
    try:
        main()
    except KeyboardInterrupt:
        pass
