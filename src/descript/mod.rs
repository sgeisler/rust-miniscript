// Script Descriptor Language
// Written in 2018 by
//     Andrew Poelstra <apoelstra@wpsoftware.net>
//
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the CC0 Public Domain Dedication
// along with this software.
// If not, see <http://creativecommons.org/publicdomain/zero/1.0/>.
//

//! # AST Tree
//!
//! Defines a variety of data structures for describing a subset of Bitcoin Script
//! which can be efficiently parsed and serialized from Script, and from which it
//! is easy to extract data needed to construct witnesses.
//!
//! Users of the library in general will only need to use the structures exposed
//! from the top level of this module; however for people wanting to do advanced
//! things, the submodules are public as well which provide visibility into the
//! components of the AST trees.
//!

use std::{fmt, str};
use std::rc::Rc;
use secp256k1;

use bitcoin::blockdata::script;
use bitcoin::blockdata::transaction::SigHashType;
use bitcoin::util::hash::Sha256dHash; // TODO needs to be sha256, not sha256d

pub mod astelem;
pub mod lex;
pub mod satisfy;

use Error;
use PublicKey;
use expression;
use self::astelem::{AstElem, parse_subexpression};
use self::lex::{lex, TokenIter};
use self::satisfy::Satisfiable;

/// Top-level script AST type
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Descript<P>(astelem::T<P>);

impl<P> From<astelem::T<P>> for Descript<P> {
    fn from(t: astelem::T<P>) -> Descript<P> {
        Descript(t)
    }
}

impl<P: fmt::Debug> fmt::Debug for Descript<P> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl<P: fmt::Display> fmt::Display for Descript<P> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Descript<secp256k1::PublicKey> {
    /// Attempt to parse a script into a Descript representation
    pub fn parse(script: &script::Script) -> Result<Descript<secp256k1::PublicKey>, Error> {
        let tokens = lex(script)?;
        let mut iter = TokenIter::new(tokens);

        let top = parse_subexpression(&mut iter)?.into_t()?;
        if let Some(leading) = iter.next() {
            Err(Error::Unexpected(leading.to_string()))
        } else {
            Ok(Descript(Rc::try_unwrap(top).expect("no outstanding refcounts")))
        }
    }

    /// Serialize back into script form
    pub fn serialize(&self) -> script::Script {
        self.0.serialize(script::Builder::new()).into_script()
    }
}

impl<P: PublicKey> Descript<P> {
    pub fn translate<F, Q, E>(&self, translatefn: &F) -> Result<Descript<Q>, E>
        where F: Fn(&P) -> Result<Q, E> {
        let inner = self.0.translate(translatefn)?;
        Ok(Descript(inner))
    }

    /// Attempt to produce a satisfying witness for the scriptpubkey represented by the parse tree
    pub fn satisfy<F, H>(&self, keyfn: Option<&F>, hashfn: Option<&H>, age: u32)
        -> Result<Vec<Vec<u8>>, Error>
        where F: Fn(&P) -> Option<(secp256k1::Signature, Option<SigHashType>)>,
              H: Fn(Sha256dHash) -> Option<[u8; 32]>
    {
        self.0.satisfy(keyfn, hashfn, age)
    }

    /// Return a list of all public keys which might contribute to satisfaction of the scriptpubkey
    pub fn required_keys(&self) -> Vec<P> {
        self.0.required_keys()
    }
}

impl<P: PublicKey> expression::FromTree for Descript<P>
    where <P as str::FromStr>::Err: ToString,
{
    /// Parse an expression tree into a descript script representation. As a general rule this should
    /// not be called directly; rather use `Descriptor::from_tree` (or better, `Descriptor::from_str`).
    fn from_tree(top: &expression::Tree) -> Result<Descript<P>, Error> {
        let inner: Rc<astelem::T<P>> = expression::FromTree::from_tree(top)?;
        Ok(Descript(Rc::try_unwrap(inner).expect("no outstanding refcounts")))
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::Descript;
    use descript::astelem::{E, W, F, V, T};

    use bitcoin::blockdata::script;
    use bitcoin::util::hash::Sha256dHash; // TODO needs to be sha256, not sha256d

    use secp256k1;

    fn pubkeys(n: usize) -> Vec<secp256k1::PublicKey> {
        let mut ret = Vec::with_capacity(n);
        let secp = secp256k1::Secp256k1::new();
        let mut sk = [0; 32];
        for i in 1..n+1 {
            sk[0] = i as u8;
            sk[1] = (i >> 8) as u8;
            sk[2] = (i >> 16) as u8;

            let pk = secp256k1::PublicKey::from_secret_key(
                &secp,
                &secp256k1::SecretKey::from_slice(&secp, &sk[..]).expect("secret key"),
            );
            ret.push(pk);
        }
        ret
    }

    fn roundtrip(tree: &Descript<secp256k1::PublicKey>, s: &str) {
        let ser = tree.serialize();
        assert_eq!(ser.to_string(), s);
        let deser = Descript::parse(&ser).expect("deserialize result of serialize");
        assert_eq!(tree, &deser);
    }

    #[test]
    fn serialize() {
        let keys = pubkeys(5);

        roundtrip(
            &Descript(T::CastE(E::CheckSig(keys[0].clone()))),
            "Script(OP_PUSHBYTES_33 028c28a97bf8298bc0d23d8c749452a32e694b65e30a9472a3954ab30fe5324caa OP_CHECKSIG)"
        );
        roundtrip(
            &Descript(T::CastE(E::CheckMultiSig(3, keys.clone()))),
            "Script(OP_PUSHNUM_3 OP_PUSHBYTES_33 028c28a97bf8298bc0d23d8c749452a32e694b65e30a9472a3954ab30fe5324caa OP_PUSHBYTES_33 03ab1ac1872a38a2f196bed5a6047f0da2c8130fe8de49fc4d5dfb201f7611d8e2 OP_PUSHBYTES_33 039729247032c0dfcf45b4841fcd72f6e9a2422631fc3466cf863e87154754dd40 OP_PUSHBYTES_33 032564fe9b5beef82d3703a607253f31ef8ea1b365772df434226aee642651b3fa OP_PUSHBYTES_33 0289637f97580a796e050791ad5a2f27af1803645d95df021a3c2d82eb8c2ca7ff OP_PUSHNUM_5 OP_CHECKMULTISIG)"
        );

        // Liquid policy
        roundtrip(
            &Descript(T::CascadeOr(
                Rc::new(E::CheckMultiSig(2, keys[0..2].to_owned())),
                Rc::new(T::And(
                     Rc::new(V::CheckMultiSig(2, keys[3..5].to_owned())),
                     Rc::new(T::Time(10000)),
                 ),
             ))),
             "Script(OP_PUSHNUM_2 OP_PUSHBYTES_33 028c28a97bf8298bc0d23d8c749452a32e694b65e30a9472a3954ab30fe5324caa \
                                  OP_PUSHBYTES_33 03ab1ac1872a38a2f196bed5a6047f0da2c8130fe8de49fc4d5dfb201f7611d8e2 \
                                  OP_PUSHNUM_2 OP_CHECKMULTISIG \
                     OP_IFDUP OP_NOTIF \
                         OP_PUSHNUM_2 OP_PUSHBYTES_33 032564fe9b5beef82d3703a607253f31ef8ea1b365772df434226aee642651b3fa \
                                      OP_PUSHBYTES_33 0289637f97580a796e050791ad5a2f27af1803645d95df021a3c2d82eb8c2ca7ff \
                                      OP_PUSHNUM_2 OP_CHECKMULTISIGVERIFY \
                         OP_PUSHBYTES_2 1027 OP_NOP3 \
                     OP_ENDIF)"
         );

        roundtrip(
            &Descript(T::Time(921)),
            "Script(OP_PUSHBYTES_2 9903 OP_NOP3)"
        );

        roundtrip(
            &Descript(T::HashEqual(Sha256dHash::from_data(&[]))),
            "Script(OP_SIZE OP_PUSHBYTES_1 20 OP_EQUALVERIFY OP_HASH256 OP_PUSHBYTES_32 5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456 OP_EQUAL)"
        );

        roundtrip(
            &Descript(T::CastE(E::CheckMultiSig(3, keys[0..5].to_owned()))),
            "Script(OP_PUSHNUM_3 \
                    OP_PUSHBYTES_33 028c28a97bf8298bc0d23d8c749452a32e694b65e30a9472a3954ab30fe5324caa \
                    OP_PUSHBYTES_33 03ab1ac1872a38a2f196bed5a6047f0da2c8130fe8de49fc4d5dfb201f7611d8e2 \
                    OP_PUSHBYTES_33 039729247032c0dfcf45b4841fcd72f6e9a2422631fc3466cf863e87154754dd40 \
                    OP_PUSHBYTES_33 032564fe9b5beef82d3703a607253f31ef8ea1b365772df434226aee642651b3fa \
                    OP_PUSHBYTES_33 0289637f97580a796e050791ad5a2f27af1803645d95df021a3c2d82eb8c2ca7ff \
                    OP_PUSHNUM_5 OP_CHECKMULTISIG)"
        );

        roundtrip(
            &Descript(T::HashEqual(Sha256dHash::from_data(&[]))),
            "Script(OP_SIZE OP_PUSHBYTES_1 20 OP_EQUALVERIFY OP_HASH256 OP_PUSHBYTES_32 5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456 OP_EQUAL)"
        );

        roundtrip(
            &Descript(T::SwitchOrV(
                Rc::new(V::CheckSig(keys[0].clone())),
                Rc::new(V::And(
                    Rc::new(V::CheckSig(keys[1].clone())),
                    Rc::new(V::CheckSig(keys[2].clone())),
                )),
            )),
            "Script(OP_IF \
                OP_PUSHBYTES_33 028c28a97bf8298bc0d23d8c749452a32e694b65e30a9472a3954ab30fe5324caa OP_CHECKSIGVERIFY \
                OP_ELSE \
                OP_PUSHBYTES_33 03ab1ac1872a38a2f196bed5a6047f0da2c8130fe8de49fc4d5dfb201f7611d8e2 OP_CHECKSIGVERIFY \
                OP_PUSHBYTES_33 039729247032c0dfcf45b4841fcd72f6e9a2422631fc3466cf863e87154754dd40 OP_CHECKSIGVERIFY \
                OP_ENDIF OP_PUSHNUM_1)"
        );

        // fuzzer
        roundtrip(
            &Descript(T::SwitchOr(
                Rc::new(T::Time(9)),
                Rc::new(T::Time(7)),
            )),
            "Script(OP_IF OP_PUSHNUM_9 OP_NOP3 OP_ELSE OP_PUSHNUM_7 OP_NOP3 OP_ENDIF)"
        );

        roundtrip(
            &Descript(T::And(
                Rc::new(V::SwitchOrT(
                    Rc::new(T::Time(9)),
                    Rc::new(T::Time(7)),
                )),
                Rc::new(T::Time(7))
            )),
            "Script(OP_IF OP_PUSHNUM_9 OP_NOP3 OP_ELSE OP_PUSHNUM_7 OP_NOP3 OP_ENDIF OP_VERIFY OP_PUSHNUM_7 OP_NOP3)"
        );

        roundtrip(
            &Descript(T::ParallelOr(
                Rc::new(E::CheckMultiSig(0, vec![])),
                Rc::new(W::CheckSig(keys[0].clone())),
            )),
            "Script(OP_0 OP_0 OP_CHECKMULTISIG OP_SWAP OP_PUSHBYTES_33 028c28a97bf8298bc0d23d8c749452a32e694b65e30a9472a3954ab30fe5324caa OP_CHECKSIG OP_BOOLOR)"
        );
    }

    #[test]
    fn deserialize() {
        // Most of these came from fuzzing, hence the increasing lengths
        assert!(Descript::parse(&script::Script::new()).is_err()); // empty script
        assert!(Descript::parse(&script::Script::from(vec![0])).is_err()); // FALSE and nothing else
        assert!(Descript::parse(&script::Script::from(vec![0x50])).is_err()); // TRUE and nothing else
        assert!(Descript::parse(&script::Script::from(vec![0x69])).is_err()); // VERIFY and nothing else
        assert!(Descript::parse(&script::Script::from(vec![0x10, 1])).is_err()); // incomplete push and nothing else
        assert!(Descript::parse(&script::Script::from(vec![0x03, 0x99, 0x03, 0x00, 0xb2])).is_err()); // non-minimal #
        assert!(Descript::parse(&script::Script::from(vec![0x85, 0x59, 0xb2])).is_err()); // leading bytes
        assert!(Descript::parse(&script::Script::from(vec![0x4c, 0x01, 0x69, 0xb2])).is_err()); // nonminimal push
        assert!(Descript::parse(&script::Script::from(vec![0x00, 0x00, 0xaf, 0x01, 0x01, 0xb2])).is_err()); // nonminimal number

        assert!(Descript::parse(&script::Script::from(vec![0x00, 0x00, 0xaf, 0x00, 0x00, 0xae, 0x85])).is_err()); // OR not BOOLOR
        assert!(Descript::parse(&script::Script::from(vec![0x00, 0x00, 0xaf, 0x00, 0x00, 0xae, 0x9b])).is_err()); // parallel OR without wrapping
    }
}
