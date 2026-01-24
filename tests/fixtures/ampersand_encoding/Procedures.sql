-- Procedure with ampersand in name
CREATE PROCEDURE [dbo].[Get_P&L_Summary]
AS
BEGIN
    SELECT * FROM [dbo].[P&L_Report];
END
GO

-- Procedure with P&I in name (like the real-world case)
CREATE PROCEDURE [dbo].[IOLoansWithoutP&IConversionNotifications]
AS
BEGIN
    SELECT 1 AS [Result];
END
GO

-- View with ampersand
CREATE VIEW [dbo].[Terms&Conditions_View]
AS
SELECT * FROM [dbo].[Terms&Conditions];
GO
