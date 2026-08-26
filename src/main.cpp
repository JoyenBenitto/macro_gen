#include <iostream>
#include "EquationTypes.hpp"
#include "Demorgan.hpp"

int main() {
    EqVar A("A");
    EqVar B("B");
    EqVar C("C");
    EqVar D("D");

    // Build your expression tree
    Node root = (A + B) * (C * D);

    std::cout << "--- BEFORE De Morgan ---\n";
    walk_ast(&root);

    // Run De Morgan transformation
    Demorgan demorgan(&root);
    demorgan.get_pos();

    std::cout << "\n--- AFTER De Morgan ---\n";
    walk_ast(&root);

    return 0;
}