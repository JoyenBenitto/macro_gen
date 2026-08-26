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

struct EqVar;

struct Node {
    opType op = opType::None;
    std::string name = ""; 
    std::vector<Node> children;

    // Overloads for EqVar on the right
    Node operator+(const EqVar& rhs) const;
    Node operator*(const EqVar& rhs) const;

    // Overloads for Node on the right (Fixes the current error!)
    Node operator+(const Node& rhs) const {
        Node n;
        n.op = opType::Or;
        n.children.push_back(*this);
        n.children.push_back(rhs);
        return n;
    }

    Node operator*(const Node& rhs) const {
        Node n;
        n.op = opType::And;
        n.children.push_back(*this);
        n.children.push_back(rhs);
        return n;
    }
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

    Node operator*(const EqVar& rhs) const {
        Node n;
        n.op = opType::And; 
        n.children.push_back(this->toNode());
        n.children.push_back(rhs.toNode());
        return n;
    }

    // Allow EqVar + Node as well, just in case
    Node operator+(const Node& rhs) const {
        Node n;
        n.op = opType::Or;
        n.children.push_back(this->toNode());
        n.children.push_back(rhs);
        return n;
    }

    Node operator*(const Node& rhs) const {
        Node n;
        n.op = opType::And;
        n.children.push_back(this->toNode());
        n.children.push_back(rhs);
        return n;
    }
};

// Implement Node + EqVar
inline Node Node::operator+(const EqVar& rhs) const {
    Node n;
    n.op = opType::Or;
    n.children.push_back(*this);
    n.children.push_back(rhs.toNode());
    return n;
}

inline Node Node::operator*(const EqVar& rhs) const {
    Node n;
    n.op = opType::And;
    n.children.push_back(*this);
    n.children.push_back(rhs.toNode());
    return n;
}

// AST Walker function
inline void walk_ast(const Node* n, int depth = 0) {
    if (!n) return;

    std::string indent(depth * 2, ' ');

    if (n->op == opType::Var) {
        std::cout << indent << "Variable: " << n->name << "\n";
    } else {
        std::string opName = "";
        if (n->op == opType::Or) opName = "OR (+)";
        else if (n->op == opType::And) opName = "AND (*)";
        else if (n->op == opType::Not) opName = "NOT";
        else opName = "NONE";

        std::cout << indent << "Operator: " << opName << "\n";

        for (const auto& child : n->children) {
            walk_ast(&child, depth + 1);
        }
    }
}

#endif