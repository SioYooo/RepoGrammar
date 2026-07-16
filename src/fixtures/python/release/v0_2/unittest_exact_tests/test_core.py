import unittest


class CoreTests(unittest.TestCase):
    def setUp(self):
        self.value = 1

    def test_one(self):
        self.assertEqual(self.value, 1)

    def test_two(self):
        self.assertTrue(self.value)

    def test_three(self):
        self.assertIsNotNone(self.value)
