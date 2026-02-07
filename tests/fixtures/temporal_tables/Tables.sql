-- Basic temporal table with PERIOD FOR SYSTEM_TIME
CREATE TABLE [dbo].[Employee] (
    [EmployeeId] INT NOT NULL PRIMARY KEY,
    [Name] NVARCHAR(100) NOT NULL,
    [Department] NVARCHAR(50) NULL,
    [Salary] DECIMAL(18, 2) NOT NULL,
    [SysStartTime] DATETIME2 GENERATED ALWAYS AS ROW START NOT NULL,
    [SysEndTime] DATETIME2 GENERATED ALWAYS AS ROW END NOT NULL,
    PERIOD FOR SYSTEM_TIME ([SysStartTime], [SysEndTime])
)
WITH (SYSTEM_VERSIONING = ON);
GO

-- Temporal table with explicit history table and HIDDEN columns
CREATE TABLE [dbo].[Product] (
    [ProductId] INT NOT NULL PRIMARY KEY,
    [ProductName] NVARCHAR(200) NOT NULL,
    [Price] DECIMAL(18, 4) NOT NULL,
    [ValidFrom] DATETIME2 GENERATED ALWAYS AS ROW START HIDDEN NOT NULL,
    [ValidTo] DATETIME2 GENERATED ALWAYS AS ROW END HIDDEN NOT NULL,
    PERIOD FOR SYSTEM_TIME ([ValidFrom], [ValidTo])
)
WITH (SYSTEM_VERSIONING = ON (HISTORY_TABLE = [dbo].[ProductHistory]));
GO

-- Non-temporal table (to verify no temporal properties are emitted)
CREATE TABLE [dbo].[Category] (
    [CategoryId] INT NOT NULL PRIMARY KEY,
    [CategoryName] NVARCHAR(100) NOT NULL
);
GO
