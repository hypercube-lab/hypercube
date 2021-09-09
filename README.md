<p align="center">
  <a href="https://hypercube-lab.github.io">
    <img alt="HyperCube" src="https://i.imgur.com/hypercube.png" width="250" />
  </a>
</p>

[![HyperCube crate](https://img.shields.io/crates/v/hypercube.svg)](https://crates.io/crates/hypercube)
[![HyperCube documentation](https://docs.rs/hypercube-lab/badge.svg)](https://docs.rs/hypercu)
[![Build status](https://badge.buildkite.com/8cc350de251d61483db98bdfc895b9ea0ac8ffa4a32ee850ed.svg?branch=master)](https://buildkite.com/hypercube-lab/hypercube/builds?branch=master)
[![codecov](https://codecov.io/gh/hypercube-lab/hypercube/branch/master/graph/badge.svg)](https://codecov.io/gh/hypercube-lab/hypercube)


# XPZ Public Chain

HyperCube is a free and open source blockchain project for everyone to use.



[HyperCube Wiki](https://github.com/hypercube-lab/hypercube/wiki)

[HyperCube Whitepaper](https://github.com/hypercube-lab/hypercube/blob/main/HyperCube.pdf)



## Wha is HyperCube

HyperCube is an Ethereum 2-layer solution based on proof of POD dedication and an independent public chain.

## Formal State

HyperCube's network is an independent public chain written in Rust language based on PoD consensus.

PoD is a new network consensus algorithm based on a hybrid consensus of PoW (ETHash) and PoS (Dedication Formula), which determines the blockchain network accounting privileges from multi-dimensional parameters such as time, network coupling, community participation, and online time.

The PoD consensus + XPZ public chain helps Ethereum increase network transaction speed and reduce Gas fees. At the same time, the introduction of the EVERNET permanent storage network can provide decentralized permanent storage space for financial infrastructure, the Internet of Things, and the Internet.


## XPZ Mining Rig

XPZ mining machine is based on the dual-core mining mode of Nvdia and AMD. Based on the special structure of PoD, XPZ can conduct joint mining with ETH, which saves energy and achieves the goal of carbon neutrality on the chain.

After XPZ upgrades the perpetual storage network, large-scale on-chain low-cost storage can be realized, which provides a guarantee for permanent storage of data by facilitating hard disk storage space.

## Origin of HyperCube

The XPZ development team is guided by multiple Turing Award winners as consultants, and the Ethereum technical team guides the development. Grayscale and Coinbase fragments. XPZ, under the joint promotion of many geek technology enthusiasts and teams around the world, actively explores the new Ethereum The road to expansion.

The XPZ (full name HyperCube) public chain supports large-scale, multi-threaded, multi-concurrent network computing and storage capabilities such as NFT casting, social tokens, DeFi, and financial applications.


## Core Advantages of HyperCube

* Support game financial GameFi, chain games, decentralized financial DeFi, XPZ system built-in Athena SDK, can help develop the rapid development of GameFi and DeFi products.

* XPZ core provides EVM general XVM (XPZ virtual machine), which is faster than EVM and enables Ethereum developers to get started quickly

* Low gas fee, fast response

* Athena SDK can provide fast casting NFT, low-cost digital art creation

* Anonymous social + OTC + qtc, XPZ qtc supports anonymous social, OTC and token storage, facilitating quick realization of NFT creators, and supporting the issuance of social tokens and personal NFT works

* XPZ supports a hybrid on-chain transaction engine based on order books and automated market makers, which can help DeFi to be implemented on a large scale

* Strong academic background: PoD and XPZ core technologies have passed peer review and will be released in international academic conferences soon

* Endorsed by IEEE and other international blockchain standard organizations

* Traditional American capital includes endorsement



***

  
# Code Coverage

To generate code coverage statistics:

```bash
$ scripts/coverage.sh
$ open target/cov/lcov-local/index.html
```

Why coverage? While most see coverage as a code quality metric, we see it primarily as a developer
productivity metric. When a developer makes a change to the codebase, presumably it's a *solution* to
some problem.  Our unit-test suite is how we encode the set of *problems* the codebase solves. Running
the test suite should indicate that your change didn't *infringe* on anyone else's solutions. Adding a
test *protects* your solution from future changes. Say you don't understand why a line of code exists,
try deleting it and running the unit-tests. The nearest test failure should tell you what problem
was solved by that code. If no test fails, go ahead and submit a Pull Request that asks, "what
problem is solved by this code?" On the other hand, if a test does fail and you can think of a
better way to solve the same problem, a Pull Request with your solution would most certainly be
welcome! Likewise, if rewriting a test can better communicate what code it's protecting, please
send us that patch!

# Disclaimer

All claims, content, designs, algorithms, estimates, roadmaps,
specifications, and performance measurements described in this project
are done with the HyperCube Lab's ("XPZ Lab") good faith efforts. It is up to
the reader to check and validate their accuracy and truthfulness.
Furthermore nothing in this project constitutes a solicitation for
investment.

Any content produced by XPZ Lab or developer resources that HyperCube Lab provides, are
for educational and inspiration purposes only. HyperCube Lab does not encourage,
induce or sanction the deployment, integration or use of any such
applications (including the code comprising the HyperCube blockchain
protocol) in violation of applicable laws or regulations and hereby
prohibits any such deployment, integration or use. This includes use of
any such applications by the reader (a) in violation of export control
or sanctions laws of the United States or any other applicable
jurisdiction, (b) if the reader is located in or ordinarily resident in
a country or territory subject to comprehensive sanctions administered
by the U.S. Office of Foreign Assets Control (OFAC), or (c) if the
reader is or is working on behalf of a Specially Designated National
(SDN) or a person subject to similar blocking or denied party
prohibitions.

The reader should be aware that U.S. export control and sanctions laws
prohibit U.S. persons (and other persons that are subject to such laws)
from transacting with persons in certain countries and territories or
that are on the SDN list. As a project based primarily on open-source
software, it is possible that such sanctioned persons may nevertheless
bypass prohibitions, obtain the code comprising the HyperCube blockchain
protocol (or other project code or applications) and deploy, integrate,
or otherwise use it. Accordingly, there is a risk to individuals that
other persons using the HyperCube blockchain protocol may be sanctioned
persons and that transactions with such persons would be a violation of
U.S. export controls and sanctions law. This risk applies to
individuals, organizations, and other ecosystem participants that
deploy, integrate, or use the HyperCube blockchain protocol code directly
(e.g., as a node operator), and individuals that transact on the HyperCube
blockchain through light clients, third party interfaces, and/or qtc
software.



