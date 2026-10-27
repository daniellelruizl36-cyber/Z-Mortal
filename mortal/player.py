import torch
import numpy as np
import os
import glob
import json
import shutil
import secrets
import logging
from os import path
from model import Brain, DQN
from engine import MortalEngine
from libriichi.stat import Stat
from libriichi.arena import OneVsThree, FspArena
from config import config

def build_engine(state_file, device, enable_compile, name):
    state = torch.load(state_file, weights_only=True, map_location=torch.device('cpu'))
    cfg = state['config']
    version = cfg['control'].get('version', 1)
    conv_channels = cfg['resnet']['conv_channels']
    num_blocks = cfg['resnet']['num_blocks']
    stable_mortal = Brain(version=version, conv_channels=conv_channels, num_blocks=num_blocks).eval()
    stable_dqn = DQN(version=version).eval()
    stable_mortal.load_state_dict(state['mortal'])
    stable_dqn.load_state_dict(state['current_dqn'])
    if enable_compile:
        stable_mortal.compile()
        stable_dqn.compile()

    return MortalEngine(
        stable_mortal,
        stable_dqn,
        is_oracle = False,
        version = version,
        device = device,
        enable_amp = True,
        enable_rule_based_agari_guard = True,
        name = name,
    )

class TestPlayer:
    def __init__(self):
        baseline_cfg = config['baseline']['test']
        device = torch.device(baseline_cfg['device'])

        state = torch.load(baseline_cfg['state_file'], weights_only=True, map_location=torch.device('cpu'))
        cfg = state['config']
        version = cfg['control'].get('version', 1)
        conv_channels = cfg['resnet']['conv_channels']
        num_blocks = cfg['resnet']['num_blocks']
        stable_mortal = Brain(version=version, conv_channels=conv_channels, num_blocks=num_blocks).eval()
        stable_dqn = DQN(version=version).eval()
        stable_mortal.load_state_dict(state['mortal'])
        stable_dqn.load_state_dict(state['current_dqn'])
        if baseline_cfg['enable_compile']:
            stable_mortal.compile()
            stable_dqn.compile()

        self.baseline_engine = MortalEngine(
            stable_mortal,
            stable_dqn,
            is_oracle = False,
            version = version,
            device = device,
            enable_amp = True,
            enable_rule_based_agari_guard = True,
            name = 'baseline',
        )
        self.chal_version = config['control']['version']
        self.log_dir = path.abspath(config['test_play']['log_dir'])

    def test_play(self, seed_count, mortal, dqn, device):
        torch.backends.cudnn.benchmark = False
        engine_chal = MortalEngine(
            mortal,
            dqn,
            is_oracle = False,
            version = self.chal_version,
            device = device,
            enable_amp = True,
            name = 'mortal',
        )

        if path.isdir(self.log_dir):
            shutil.rmtree(self.log_dir)

        env = OneVsThree(
            disable_progress_bar = False,
            log_dir = self.log_dir,
        )
        env.py_vs_py(
            challenger = engine_chal,
            champion = self.baseline_engine,
            seed_start = (10000, 0x2000),
            seed_count = seed_count,
        )

        stat = Stat.from_dir(self.log_dir, 'mortal')
        torch.backends.cudnn.benchmark = config['control']['enable_cudnn_benchmark']
        return stat

class TrainPlayer:
    def __init__(self):
        baseline_cfg = config['baseline']['train']
        device = torch.device(baseline_cfg['device'])

        self.baseline_engine = build_engine(
            baseline_cfg['state_file'],
            device,
            baseline_cfg['enable_compile'],
            'baseline',
        )

        profile = os.environ.get('TRAIN_PLAY_PROFILE', 'default')
        logging.info(f'using profile {profile}')
        cfg = config['train_play'][profile]
        self.chal_version = config['control']['version']
        self.log_dir = path.abspath(cfg['log_dir'])
        # Multiple concurrent clients must not share a log dir: each one's
        # train_play rmtrees it, which would corrupt the others' in-flight
        # games and submitted logs.
        client_id = os.environ.get('MORTAL_CLIENT_ID')
        if client_id:
            self.log_dir = path.join(self.log_dir, client_id)
        self.train_key = secrets.randbits(64)
        self.train_seed = 10000

        self.seed_count = cfg['games'] // 4
        self.boltzmann_epsilon = cfg['boltzmann_epsilon']
        self.boltzmann_temp = cfg['boltzmann_temp']
        self.top_p = cfg['top_p']

        self.repeats = cfg['repeats']
        self.repeat_counter = 0

        pool_cfg = config.get('fsp_pool', {})
        self.champion_dir = pool_cfg.get('champion_dir')
        self.n_older = pool_cfg.get('n_older', 1)
        self.seat_randomize = pool_cfg.get('seat_randomize', True)
        self.rule = pool_cfg.get('rule')
        self.cu_cfg = config['env'].get('cu', {})

    def _load_pool_engines(self):
        if not self.champion_dir or not path.isdir(self.champion_dir):
            return []
        files = sorted(
            glob.glob(path.join(self.champion_dir, '*.pth')),
            key=os.path.getmtime,
            reverse=True,
        )
        return [
            build_engine(f, self.baseline_engine.device, False, f'champ_{i}')
            for i, f in enumerate(files)
        ]

    def train_play(self, mortal, dqn, device):
        torch.backends.cudnn.benchmark = False
        engine_chal = MortalEngine(
            mortal,
            dqn,
            is_oracle = False,
            version = self.chal_version,
            boltzmann_epsilon = self.boltzmann_epsilon,
            boltzmann_temp = self.boltzmann_temp,
            top_p = self.top_p,
            device = device,
            enable_amp = True,
            name = 'trainee',
        )

        if path.isdir(self.log_dir):
            shutil.rmtree(self.log_dir)

        pool = self._load_pool_engines()
        if pool:
            rankings, avg_cu, cus = self._play_fsp(engine_chal, pool)
        else:
            rankings, cus = self._play_one_vs_three(engine_chal), []
        # per-game challenger CU of the most recent session, consumed by
        # client.py for the promotion gate
        self.last_game_cus = cus

        self.repeat_counter += 1
        if self.repeat_counter == self.repeats:
            self.train_seed += self.seed_count
            self.repeat_counter = 0

        rankings = np.array(rankings)
        file_list = list(map(lambda p: path.join(self.log_dir, p), os.listdir(self.log_dir)))

        torch.backends.cudnn.benchmark = config['control']['enable_cudnn_benchmark']
        return rankings, file_list

    def _play_one_vs_three(self, engine_chal):
        env = OneVsThree(
            disable_progress_bar = False,
            log_dir = self.log_dir,
        )
        return env.py_vs_py(
            challenger = engine_chal,
            champion = self.baseline_engine,
            seed_start = (self.train_seed, self.train_key),
            seed_count = self.seed_count,
        )

    def _play_fsp(self, engine_chal, pool):
        env = FspArena(
            disable_progress_bar = False,
            log_dir = self.log_dir,
            rule = json.dumps(self.rule) if isinstance(self.rule, dict) else self.rule,
        )
        champ_older = pool[min(1, len(pool) - 1)] if self.n_older > 0 else None
        records = env.py_vs_py(
            challenger = engine_chal,
            champ_latest = pool[0],
            champ_older = champ_older,
            seed_start = (self.train_seed, self.train_key),
            seed_count = self.seed_count * 4,
            n_older = self.n_older,
            seat_randomize = self.seat_randomize,
        )

        pt_scale = self.cu_cfg.get('pt_scale', 0.01)
        zhuyi_value = self.cu_cfg.get('zhuyi_value', 20)
        uma = self.cu_cfg.get('uma', [450, 50, -150, -350])

        rankings = [0, 0, 0, 0]
        cus = []
        for seat, scores, zhuyi in records:
            scores = np.asarray(scores)
            rank = int(np.sum(scores > scores[seat]))
            cu = (scores[seat] - 25000) * pt_scale + zhuyi[seat] * zhuyi_value + uma[rank]
            cus.append(cu)
            rankings[rank] += 1
        avg_cu = float(np.mean(cus)) if cus else 0.0
        logging.info(f'fsp avg CU: {avg_cu:.1f} over {len(cus)} games, rankings: {rankings}')
        return rankings, avg_cu, cus
