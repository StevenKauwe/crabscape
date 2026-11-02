import unittest
from model import Conflict, Store


class ChunkStoreProtocol(unittest.TestCase):
    def test_unpublished_chunks_never_replace_old_save_and_can_be_collected(self):
        s = Store()
        e = s.begin('a', 5, b'old')
        s.stage('a', e, [('a/0', b'old terrain')])
        s.publish('a', e, 1)
        e = s.begin('b', 8, b'new')
        s.stage('b', e, [('b/0', b'partial')])
        s.collect()
        self.assertEqual(s.read()[1], (b'old terrain',))
        s.abandon('b', e)
        s.collect()
        self.assertEqual(set(s.blobs), {'a/0'})

    def test_reuse_and_collect_keep_live_shared_chunks(self):
        s = Store()
        e = s.begin('a', 1, b'p')
        s.stage('a', e, [('same', b'unchanged'), ('old', b'old')])
        s.publish('a', e, 2)
        e = s.begin('b', 2, b'q')
        s.stage('b', e, [('same', b'unchanged'), ('new', b'new')])
        s.collect()
        s.publish('b', e, 2)
        s.collect()
        self.assertEqual(s.read()[1], (b'unchanged', b'new'))
        self.assertEqual(set(s.blobs), {'same', 'new'})

    def test_reset_invalidates_pending_cross_tab_writer_even_after_new_save(self):
        s = Store(max_pending=2)
        stale = s.begin('stale', 1, b'old')
        s.stage('stale', stale, [('x', b'x')])
        s.reset()
        current = s.begin('new', 2, b'new')
        s.stage('new', current, [('y', b'y')])
        s.publish('new', current, 1)
        for op in (lambda: s.stage('stale', stale, [('z', b'z')]),
                   lambda: s.publish('stale', stale, 1),
                   lambda: s.abandon('stale', stale)):
            with self.assertRaises(Conflict):
                op()
        self.assertEqual(s.read()[1], (b'y',))

    def test_competing_publish_and_incomplete_snapshot_never_replace_head(self):
        s = Store(max_pending=2)
        a = s.begin('a', 1, b'a')
        b = s.begin('b', 2, b'b')
        s.stage('a', a, [('a', b'a')])
        s.stage('b', b, [('b', b'b')])
        with self.assertRaises(Conflict):
            s.publish('a', a, 2)
        self.assertIsNone(s.read())
        s.publish('a', a, 1)
        with self.assertRaises(Conflict):
            s.publish('b', b, 1)
        self.assertEqual(s.read()[1], (b'a',))

    def test_backpressure_immutability_and_batch_atomicity(self):
        s = Store(max_batch=2)
        e = s.begin('a', 1, b'p')
        with self.assertRaises(Conflict):
            s.begin('b', 2, b'q')
        with self.assertRaises(Conflict):
            s.stage('a', e, [('a', b'a'), ('b', b'b'), ('c', b'c')])
        self.assertEqual(s.blobs, {})
        s.stage('a', e, [('a', b'a')])
        with self.assertRaises(Conflict):
            s.stage('a', e, [('b', b'b'), ('a', b'changed')])
        self.assertEqual(s.blobs, {'a': b'a'})
        self.assertEqual(s.stages['a'].chunks, ['a'])


if __name__ == '__main__':
    unittest.main()
