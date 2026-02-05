-- Math utilities module

local M = {}

function M.add(a, b)
    return a + b
end

function M.multiply(a, b)
    return a * b
end

function M.factorial(n)
    if n <= 1 then
        return 1
    end
    return n * M.factorial(n - 1)
end

return M
