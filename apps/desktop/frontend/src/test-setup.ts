const reactActGlobal = globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean };

reactActGlobal.IS_REACT_ACT_ENVIRONMENT = true;
