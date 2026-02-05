-- Test module loading in CopperMoon

local math_utils = require("lib.math_utils")

print("Testing module loading...")

print("2 + 3 =", math_utils.add(2, 3))
print("4 * 5 =", math_utils.multiply(4, 5))
print("5! =", math_utils.factorial(5))

print("Module loading works!")
