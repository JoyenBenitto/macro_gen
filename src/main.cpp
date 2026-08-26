#include <iostream>
#include "EquationTypes.hpp"

int main() {
    std::cout << "Hello, World!" << std::endl;

    EqVar A("A");
    EqVar B("B");
    EqVar C("C");

    // This will now compile and build the nested AST successfully!
    auto expressionRoot = (A + B) + C;

    std::cout << "\nGenerated AST Structure:\n";
    walk_ast(&expressionRoot);
    return 0;
}