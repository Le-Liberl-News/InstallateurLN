"""Pure-Python stand-in for the `subsetsum` package dsretrack.py imports (the
Windows build ships it compiled, _subsetsum.pyd; Linux has nothing to install).

dsretrack only does `list(solutions(lengths, target))[0]`: the indices of texts
whose lengths add up exactly to `target`, in increasing order. Enumerating every
solution can explode, so this yields the first one only (dynamic programming
over the sums), which is all the script uses.
"""


def solutions(lengths, target):
    if target < 0:
        return
    # reach[s] = index of the item that first completed sum s (-1: the empty sum)
    reach = {0: -1}
    for index, length in enumerate(lengths):
        if length <= 0:
            continue
        for total in sorted(reach, reverse=True):
            new = total + length
            if new <= target and new not in reach:
                reach[new] = index
        if target in reach:
            break
    if target not in reach or target == 0:
        return
    chosen = []
    total = target
    while total != 0:
        index = reach[total]
        chosen.append(index)
        total -= lengths[index]
    yield sorted(chosen)
