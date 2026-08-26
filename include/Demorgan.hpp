#ifndef DEMORG
#define DEMORG

#include <iostream>
#include "EquationTypes.hpp"

class Demorgan {
    private:
        Node *n;
        Node *pos;
        Node *sop;

        Node gen_pos(const Node& node) {
            // 1. Base case: Return the variable AS-IS. 
            // The PMOS transistor itself handles the inversion physically!
            if (node.op == opType::Var) {
                return node;
            }
            
            Node new_node = node;
            new_node.children.clear(); 

            // 2. Flip the operators (This creates the dual topology for the PUN)
            if (node.op == opType::And) {
                new_node.op = opType::Or;
            } else if (node.op == opType::Or) {
                new_node.op = opType::And;
            }

            // 3. Recursively transform all children and save them
            for (const auto& child : node.children) {
                new_node.children.push_back(gen_pos(child));
            }

            return new_node;
    }

    public:
        Demorgan (Node *node_ptr) {
            this->n = node_ptr;
            this->pos = nullptr;
            this->sop = nullptr;
        }

        Node* get_sop(){
            return sop;
        }

        /*
        Getter for pos / transformed tree
        */
        Node* get_pos(){
            if (n) {
                // Run the transformation and store it back into the root node
                *n = gen_pos(*n);
            }
            return this->n;
        }
};

#endif