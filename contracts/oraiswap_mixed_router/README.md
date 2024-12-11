# Oraiswap Router

The Router Contract contains the logic to facilitate multi-hop swap operations via oraiswap.

**On-chain swap & Oraiswap is supported.**

Oraiswap Router Contract:

- https://scan.orai.io/address/orai14z80rwpd0alzj4xdtgqdmcqt9wd9xj5ffd60wp

Tx:

- KRT => Orai => mABNB: https://scan.orai.io/tx/46A1C956D2F4F7A1FA22A8F93749AEADB953ACDFC1B9FB7661EEAB5C59188175
- mABNB => Orai => KRT: https://scan.orai.io/tx/E9D63CE2C8AC38F6C9434C62F9A8B59F38259FEB86F075D43C253EA485D7F0A9

### Operations Assertion

The contract will check whether the resulting token is swapped into one token.

### Example

Swap KRT => Orai => mABNB
If the token is ow20 Orai, we can convert it to native first

```
{
   "execute_swap_operations":{
      "operations":[
         {
            "orai_swap":{
               "offer_asset_info":{
                  "token":{
                     "contract_addr":"orai1avryzxnsn2denq7p2d7ukm6nkck9s0rz2llgnc"
                  }
               },
               "ask_asset_info":{
                  "native_token":{
                     "denom":"orai"
                  }
               }
            }
         },
         {
            "orai_swap":{
               "offer_asset_info":{
                  "native_token":{
                     "denom":"orai"
                  }
               },
               "ask_asset_info":{
                  "token":{
                     "contract_addr":"orai1avryzxnsn2denq7p2d7ukm6nkck9s0rz2llgnc"
                  }
               }
            }
         }
      ],
      "minimum_receive":"88000"
   }
}
```
