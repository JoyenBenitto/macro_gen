#ifndef EQUATIONTYPES_HPP
#define EQUATIONTYPES_HPP

#include <string>
#include <vector>
#include <iostream>

enum class opType {
    And,
    Or,
    Not,
    None,
    Var
};

// Forward declaration so Node knows EqVar exists
struct EqVar;

struct Node {
    opType op = opType::None;
    std::string name = ""; 
    std::vector<Node> children;

    // Node + EqVar chaining support
    Node operator+(const EqVar& rhs) const;
};

struct EqVar {
    std::string name;
    
    EqVar(std::string n) : name(n) {}

    Node toNode() const {
        return Node{opType::Var, name, {}};
    }

    Node operator+(const EqVar& rhs) const {
        Node n;
        n.op = opType::Or; 
        n.children.push_back(this->toNode());
        n.children.push_back(rhs.toNode());
        return n;
    }
};

// Now implement Node + EqVar down here where EqVar is fully defined
inline Node Node::operator+(const EqVar& rhs) const {
    Node n;
    n.op = opType::Or;
    n.children.push_back(*this); // 'this' node is already a Node!
    n.children.push_back(rhs.toNode());
    return n;
}

void walk_ast(const Node* n, int depth = 0) {
    if (!n) return;

    // Create an indentation string based on depth for nice visual hierarchy
    std::string indent(depth * 2, ' ');

    // Check what kind of node it is
    if (n->op == opType::Var) {
        std::cout << indent << "Variable: " << n->name << "\n";
    } else {
        // Print the operator type
        std::string opName = "";
        if (n->op == opType::Or) opName = "OR (+)";
        else if (n->op == opType::And) opName = "AND";
        else if (n->op == opType::Not) opName = "NOT";
        else opName = "NONE";

        std::cout << indent << "Operator: " << opName << "\n";

        // Recursively walk all children
        for (const auto& child : n->children) {
            walk_ast(&child, depth + 1);
        }
    }
}

#endif