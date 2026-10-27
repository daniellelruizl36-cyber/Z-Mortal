import json
import os
import traceback
import torch
import numpy as np
from torch.distributions import Normal, Categorical
from typing import *

# VB1 branch B 锁（aka=3 门清锁）：MORTAL_LOCK_AKA3=1 时启用。
# 行为侧 mask（本文件）必须与训练侧 mask（dataloader_vrpo.populate_game）一致，
# 否则行为似然与训练 mask 错位（naive-masking 管道版）。
# 观测布局（version=3，逐节求和自 obs_repr.rs encode_obs，vb1_calibrate_lock.py 实证校验 PASS）：
#   tehai 4 行 [0,4)；akas_in_hand 3 行 one-hot [4,7)（fill=整行填 1，行和/34=红宝数）
#   自家 fuuro_overview [796,816)（4 槽 x 5 行；空即门清；ankan 在独立块，门清语义正确）
#   布局全链：tehai4 aka3 scores40 rank4 kyoku4 honba2 kyotaku2 winds2 bakyoku1
#   dora7 selkawa96+1 opp kawa 3x(192+3) tilesleft1 doras12+4 kawaov28 fuuroov80 ankan4
#   tilesseen1 tedashi9 risute9 rdec3 racc3 waits1 furiten1 shanten7 riacc1 kansel1
#   pass3 discard5 reach1 chi3 pon1 dmink1 ankan1 kakan1 hora1 ryuk1 = 934 ✓
_LOCK_AKA3 = os.environ.get('MORTAL_LOCK_AKA3', '') == '1'
_V3_OBS_ROWS = 934
_AKA_ROWS = (4, 7)
_FUURO_SELF = (796, 816)
_CALL_ACTIONS = (38, 39, 40, 41)

def lock_aka3_enabled():
    return _LOCK_AKA3

class MortalEngine:
    def __init__(
        self,
        brain,
        dqn,
        is_oracle,
        version,
        device = None,
        stochastic_latent = False,
        enable_amp = False,
        enable_quick_eval = True,
        enable_rule_based_agari_guard = False,
        name = 'NoName',
        boltzmann_epsilon = 0,
        boltzmann_temp = 1,
        top_p = 1,
        adapter = None,
    ):
        self.engine_type = 'mortal'
        self.device = device or torch.device('cpu')
        assert isinstance(self.device, torch.device)
        self.brain = brain.to(self.device).eval()
        self.dqn = dqn.to(self.device).eval()
        self.adapter = adapter.to(self.device).eval() if adapter is not None else None
        self.is_oracle = is_oracle
        self.version = version
        self.stochastic_latent = stochastic_latent

        self.enable_amp = enable_amp
        self.enable_quick_eval = enable_quick_eval
        self.enable_rule_based_agari_guard = enable_rule_based_agari_guard
        self.name = name

        self.boltzmann_epsilon = boltzmann_epsilon
        self.boltzmann_temp = boltzmann_temp
        self.top_p = top_p
        self.lock_aka3_menzen = _LOCK_AKA3

    def _apply_aka3_lock(self, obs, masks):
        if obs.shape[1] != _V3_OBS_ROWS:
            raise RuntimeError(
                f'lock_aka3_menzen requires obs version 3 ({_V3_OBS_ROWS} rows), '
                f'got {obs.shape[1]}'
            )
        aka = obs[:, _AKA_ROWS[0]:_AKA_ROWS[1], :].sum(dim=(1, 2)) / 34.0
        fuuro = obs[:, _FUURO_SELF[0]:_FUURO_SELF[1], :].sum(dim=(1, 2))
        locked = (aka >= 3) & (fuuro == 0)
        if not locked.any():
            return masks
        m = masks.clone()
        call_cols = torch.zeros(m.shape[1], dtype=torch.bool, device=m.device)
        call_cols[list(_CALL_ACTIONS)] = True
        m[locked] = m[locked] & ~call_cols
        if not m[locked].any(-1).all():
            raise RuntimeError('aka3 lock emptied an all-call mask row (no pass fallback)')
        self._lock_hits = getattr(self, '_lock_hits', 0) + int(locked.sum())
        return m

    def react_batch(self, obs, masks, invisible_obs):
        try:
            with (
                torch.autocast(self.device.type, enabled=self.enable_amp),
                torch.inference_mode(),
            ):
                return self._react_batch(obs, masks, invisible_obs)
        except Exception as ex:
            raise Exception(f'{ex}\n{traceback.format_exc()}')

    def _react_batch(self, obs, masks, invisible_obs):
        obs = torch.as_tensor(np.stack(obs, axis=0), device=self.device)
        masks = torch.as_tensor(np.stack(masks, axis=0), device=self.device)
        if invisible_obs is not None:
            invisible_obs = torch.as_tensor(np.stack(invisible_obs, axis=0), device=self.device)
        batch_size = obs.shape[0]

        if self.lock_aka3_menzen:
            masks = self._apply_aka3_lock(obs, masks)

        match self.version:
            case 1:
                mu, logsig = self.brain(obs, invisible_obs)
                if self.stochastic_latent:
                    latent = Normal(mu, logsig.exp() + 1e-6).sample()
                else:
                    latent = mu
                q_out = self.dqn(latent, masks)
            case 2 | 3 | 4:
                phi = self.brain(obs, invisible_obs)
                if self.adapter is not None:
                    # O1: closed-eye rows (invisible_obs=None) get a zero inv so
                    # the same weights serve open/closed tests; dqn in_dim must
                    # match 1024 + adapter.out_dim.
                    if invisible_obs is None:
                        invisible_obs = torch.zeros(
                            batch_size, 217, 34, device=self.device)
                    phi = torch.cat([phi, self.adapter(invisible_obs)], dim=-1)
                q_out = self.dqn(phi, masks)

        if self.boltzmann_epsilon > 0:
            is_greedy = torch.full((batch_size,), 1-self.boltzmann_epsilon, device=self.device).bernoulli().to(torch.bool)
            logits = (q_out / self.boltzmann_temp).masked_fill(~masks, -torch.inf)
            sampled = sample_top_p(logits, self.top_p)
            actions = torch.where(is_greedy, q_out.argmax(-1), sampled)
        else:
            is_greedy = torch.ones(batch_size, dtype=torch.bool, device=self.device)
            actions = q_out.argmax(-1)

        return actions.tolist(), q_out.tolist(), masks.tolist(), is_greedy.tolist()

def sample_top_p(logits, p):
    if p >= 1:
        return Categorical(logits=logits).sample()
    if p <= 0:
        return logits.argmax(-1)
    probs = logits.softmax(-1)
    probs_sort, probs_idx = probs.sort(-1, descending=True)
    probs_sum = probs_sort.cumsum(-1)
    mask = probs_sum - probs_sort > p
    probs_sort[mask] = 0.
    sampled = probs_idx.gather(-1, probs_sort.multinomial(1)).squeeze(-1)
    return sampled

class ExampleMjaiLogEngine:
    def __init__(self, name: str):
        self.engine_type = 'mjai-log'
        self.name = name
        self.player_ids = None

    def set_player_ids(self, player_ids: List[int]):
        self.player_ids = player_ids

    def react_batch(self, game_states):
        res = []
        for game_state in game_states:
            game_idx = game_state.game_index
            state = game_state.state
            events_json = game_state.events_json

            events = json.loads(events_json)
            assert events[0]['type'] == 'start_kyoku'

            player_id = self.player_ids[game_idx]
            cans = state.last_cans
            if cans.can_discard:
                tile = state.last_self_tsumo()
                res.append(json.dumps({
                    'type': 'dahai',
                    'actor': player_id,
                    'pai': tile,
                    'tsumogiri': True,
                }))
            else:
                res.append('{"type":"none"}')
        return res

    # They will be executed at specific events. They can be no-op but must be
    # defined.
    def start_game(self, game_idx: int):
        pass
    def end_kyoku(self, game_idx: int):
        pass
    def end_game(self, game_idx: int, scores: List[int]):
        pass
