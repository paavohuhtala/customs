
import { ExportedType } from "./test_exports"

const firstName = "Peter"
const lastName = "Peterson"

export type PersonType = { firstName: string }

interface PersonInterface { firstName: string }

type Parametrised<T> = T

type GenericPerson = Parametrised<{ firstName: string }>

export interface DerivedPerson extends PersonInterface { }

export function innerScope() {
  const firstName = "Different name!"

  type PersonType = {
    shadowedType: "this is fine!"
  }

  type OuterTypeReference = DerivedPerson;

  const outerValueReference = lastName;
}
