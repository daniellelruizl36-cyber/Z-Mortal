"""Convert the stage-1 pretrained checkpoint into a fresh stage-2 online init:
stage-1 mortal/dqn weights + a fresh 1-dim kyoku-EV aux head + zeroed
bookkeeping. The embedded config is marked offline so train.py skips the
optimizer/scheduler restore (fresh stage-2 optimization).

Usage: python make_stage2_init.py <src.pth> <dst.pth>
"""
import prelude

import sys
import time
import copy
import torch
from model import AuxNet
from config import config

def main():
    src, dst = sys.argv[1], sys.argv[2]
    old = torch.load(src, weights_only=True, map_location='cpu')

    aux = AuxNet((1,))
    embedded = copy.deepcopy(config)
    embedded['control']['online'] = False # => train.py restores weights only

    state = {
        'mortal': old['mortal'],
        'current_dqn': old['current_dqn'],
        'aux_net': aux.state_dict(),
        'scaler': old['scaler'],
        'best_perf': {'avg_rank': 4.0, 'avg_pt': -135.0},
        'steps': 0,
        'timestamp': time.time(),
        'config': embedded,
    }
    torch.save(state, dst)
    print(f'saved {dst} (steps=0, aux head reinitialized)')

if __name__ == '__main__':
    main()
