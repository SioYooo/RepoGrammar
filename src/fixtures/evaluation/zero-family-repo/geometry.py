"""Plain arithmetic helpers with no framework imports.

This fixture deliberately contains only ordinary functions so that the
product-core evaluation harness can observe how RepoGrammar behaves on a
repository where no pattern-family evidence can exist.
"""


def rectangle_area(width, height):
    return width * height


def rectangle_perimeter(width, height):
    return 2 * (width + height)


def triangle_area(base, height):
    return 0.5 * base * height
