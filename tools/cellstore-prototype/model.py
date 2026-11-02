"""Executable transaction model, not an IndexedDB implementation.

Every public operation is one serializable transaction over a single store.
The production backend must provide those same boundaries, including for GC.
"""
from dataclasses import dataclass, field


class Conflict(Exception):
    pass


@dataclass(frozen=True)
class Head:
    revision: int
    tick: int
    player: bytes
    chunks: tuple[str, ...]


@dataclass
class Stage:
    epoch: int
    base_revision: int
    tick: int
    player: bytes
    chunks: list[str] = field(default_factory=list)


class Store:
    def __init__(self, max_batch=4, max_pending=1):
        self.epoch = 0
        self.revision = 0
        self.head = None
        self.blobs = {}
        self.stages = {}
        self.max_batch = max_batch
        self.max_pending = max_pending

    def begin(self, stage_id, tick, player):
        if stage_id in self.stages or len(self.stages) >= self.max_pending:
            raise Conflict('writer busy')
        self.stages[stage_id] = Stage(self.epoch, self.revision, tick, player)
        return self.epoch

    def stage(self, stage_id, epoch, entries):
        stage = self._current(stage_id, epoch)
        if len(entries) > self.max_batch:
            raise Conflict('batch too large')
        # Validate the entire batch before committing any part of it.
        combined = dict(self.blobs)
        seen = set(stage.chunks)
        for key, value in entries:
            if key in seen or key in combined and combined[key] != value:
                raise Conflict('duplicate or mutated immutable chunk')
            seen.add(key)
            combined[key] = value
        self.blobs = combined
        stage.chunks.extend(key for key, _ in entries)

    def publish(self, stage_id, epoch, expected_count):
        stage = self._current(stage_id, epoch)
        if stage.base_revision != self.revision or len(stage.chunks) != expected_count:
            raise Conflict('stale or incomplete snapshot')
        if any(key not in self.blobs for key in stage.chunks):
            raise Conflict('missing chunk')
        self.revision += 1
        self.head = Head(self.revision, stage.tick, stage.player, tuple(stage.chunks))
        del self.stages[stage_id]

    def abandon(self, stage_id, epoch):
        self._current(stage_id, epoch)
        del self.stages[stage_id]

    def collect(self):
        # A production collector must compute reachability and delete in the
        # same transaction, excluding both published and in-flight references.
        reachable = set(self.head.chunks if self.head else ())
        for stage in self.stages.values():
            reachable.update(stage.chunks)
        self.blobs = {key: value for key, value in self.blobs.items() if key in reachable}

    def reset(self):
        # Epoch advance and clear must commit atomically, and the epoch must
        # remain stored even when all other records are removed.
        self.epoch += 1
        self.revision += 1
        self.head = None
        self.blobs.clear()
        self.stages.clear()

    def read(self):
        # One consistent read transaction, not separately opened reads.
        if self.head is None:
            return None
        return self.head, tuple(self.blobs[key] for key in self.head.chunks)

    def _current(self, stage_id, epoch):
        stage = self.stages.get(stage_id)
        if epoch != self.epoch or stage is None or stage.epoch != epoch:
            raise Conflict('stale writer')
        return stage
